//! How far the device that has a chat's conversation has read each reply out, and how each ended: the place it's
//! reading at, live, and the ending it says, or the hub settles once the device goes or the mode ends.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{ClientDevice, Ending, MessageId, SettledBy, SpokenThrough};
use tokio::sync::watch;
use tracing::error;

use super::conversation::has_the_conversation;
use super::{LiveItem, LiveSession, LiveState, publish};
use crate::hub::Hub;

/// The connection a device reads a reply out on, so the hub can settle the reply once it closes.
pub(crate) struct ReadingOn {
    /// Which connection it is, of every one the hub has served.
    pub(crate) connection: u64,
    /// True once the connection reads nothing more.
    pub(crate) closed: watch::Receiver<bool>,
}

/// What the live session holds of the reply being read out.
#[derive(Debug, Default)]
pub(super) struct ReadingOut {
    /// How far the reply has been read, and on which connection, as the device last said, until it says how the reply
    /// ended, or the hub settles it. Each subscriber's tail has the place, as [`protocol::tail::Tail::speaking`] says.
    speaking: Option<Speaking>,
    /// The connection a task waits on to close, so there's one for each connection, however many replies it reads.
    watching: Option<u64>,
}

#[derive(Debug)]
struct Speaking {
    through: SpokenThrough,
    connection: u64,
}

impl ReadingOut {
    /// The place the tail says a reply has been read to.
    pub(super) fn speaking(&self) -> Option<SpokenThrough> {
        self.speaking.as_ref().map(|speaking| speaking.through.clone())
    }
}

/// Stops the tail saying a reply is being read, if `which` says this one is, and gives back where it had got to.
fn stop_speaking(state: &mut LiveState, which: impl FnOnce(&Speaking) -> bool) -> Option<SpokenThrough> {
    let speaking = state.reading_out.speaking.take_if(|speaking| which(speaking))?;
    publish(state, LiveItem::Rewritten);
    Some(speaking.through)
}

impl LiveSession {
    /// Notes how far `device` has read the agent's reply out, on the connection `on`, as
    /// [`protocol::client::conversation::SessionSpeaking`] says.
    pub(crate) async fn speaking(
        self: &Arc<Self>,
        device: Option<&ClientDevice>,
        on: ReadingOn,
        through: SpokenThrough,
    ) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let _settling = self.settling.lock().await;
        has_the_conversation(&self.state.lock().interaction, device)?;
        let (id, message) = (self.id.clone(), through.message.clone());
        // What the hub settled, as the device's connection closed, the device can go on from, as it can put it right.
        let said = hub.blocking(move |store| store.reply_heard(&id, &message)).await?;
        if said.is_some_and(|heard| heard.settled_by.is_device()) {
            return Ok(());
        }
        let watch = {
            let mut state = self.state.lock();
            let reading_out = &mut state.reading_out;
            let behind = reading_out.speaking.as_ref().is_some_and(|speaking| {
                speaking.through.message == through.message && speaking.through.through >= through.through
            });
            if behind {
                return Ok(());
            }
            reading_out.speaking = Some(Speaking { through, connection: on.connection });
            let watch = reading_out.watching.replace(on.connection) != Some(on.connection);
            publish(&mut state, LiveItem::Rewritten);
            watch
        };
        if watch {
            let session = Arc::downgrade(self);
            let ReadingOn { connection, mut closed } = on;
            tokio::spawn(async move {
                // A connection that's gone without saying so is closed as well.
                let _ = closed.wait_for(|closed| *closed).await;
                if let Some(session) = session.upgrade() {
                    session.connection_closed(connection).await;
                }
            });
        }
        Ok(())
    }

    /// Keeps how the reply `message` ended for the person hearing it, as `device` read it out, which has to have the
    /// conversation, and tells everyone following the feed, unless how it ended is kept already.
    pub(crate) async fn keep_reply_heard(
        &self,
        device: Option<&ClientDevice>,
        message: MessageId,
        ending: Ending,
    ) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let _settling = self.settling.lock().await;
        has_the_conversation(&self.state.lock().interaction, device)?;
        stop_speaking(&mut self.state.lock(), |speaking| speaking.through.message == message);
        self.keep(&hub, message, ending, SettledBy::Device).await
    }

    /// Settles the reply being read on `connection`, which has closed, as stopped where it had got to.
    async fn connection_closed(&self, connection: u64) {
        let Ok(hub) = self.hub() else { return };
        let _settling = self.settling.lock().await;
        let stopped = {
            let mut state = self.state.lock();
            state.reading_out.watching.take_if(|watching| *watching == connection);
            stop_speaking(&mut state, |speaking| speaking.connection == connection)
        };
        if let Some(through) = stopped
            && let Err(e) =
                self.keep(&hub, through.message, Ending::Stopped { through: through.through }, SettledBy::Hub).await
        {
            error!(
                name: "session.reading_out_unsettled",
                session_id = %self.id,
                error = %e,
                "Couldn't settle the reply being read out as its connection closed"
            );
        }
    }

    /// Settles every reply the device that had the conversation hadn't said the end of, since it won't now: the conversation
    /// has ended or moved. The one being read is stopped where it had got to, and every other the agent wrote in the
    /// mode with something to read out wasn't read out.
    pub(super) async fn settle_hearing(&self, hub: &Hub) {
        let _settling = self.settling.lock().await;
        if let Err(e) = self.settle_each(hub).await {
            error!(
                name: "session.reading_out_unsettled",
                session_id = %self.id,
                error = %e,
                "Couldn't settle the replies the conversation hadn't read out"
            );
        }
    }

    async fn settle_each(&self, hub: &Hub) -> Result<(), ApiError> {
        let stopped = stop_speaking(&mut self.state.lock(), |_| true);
        if let Some(through) = stopped {
            self.keep(hub, through.message, Ending::Stopped { through: through.through }, SettledBy::Hub).await?;
        }
        let id = self.id.clone();
        for message in hub.blocking(move |store| store.replies_unheard(&id)).await? {
            self.keep(hub, message, Ending::NotReadOut, SettledBy::Hub).await?;
        }
        Ok(())
    }

    /// Keeps how `message` ended, as `by` says, and tells everyone following the feed if that changed it. For a caller
    /// holding `settling`.
    async fn keep(&self, hub: &Hub, message: MessageId, ending: Ending, by: SettledBy) -> Result<(), ApiError> {
        let id = self.id.clone();
        if let Some(heard) = hub.blocking(move |store| store.keep_reply_heard(&id, &message, ending, by)).await? {
            hub.notify(HubNotice::ReplyHeard(heard));
        }
        Ok(())
    }
}
