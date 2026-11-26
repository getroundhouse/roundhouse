use std::sync::Arc;

use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaved};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolServerSaveId;
use tokio::sync::watch;

use super::super::{Hub, off_thread};
use super::saves::Save;
use crate::tool_servers::{Kept, Saving, When};

impl Hub {
    /// Begins saving a tool server, as `ToolServerSave` says, as its connection reads it, so a cancel the connection
    /// sends after it finds it, and answers with the rest, which goes on while the connection reads on. That's let go of
    /// once `closed` says the connection has closed, or its sender has gone, before the hub begins to save it, as if it
    /// had been cancelled, since nobody is left to say it was saved, or to cancel it.
    pub(crate) fn tool_server_save(
        self: &Arc<Self>,
        asked: ToolServerSave,
        closed: watch::Receiver<bool>,
    ) -> Result<impl Future<Output = Result<ToolServerSaved, ApiError>> + Send + 'static, ApiError> {
        let ToolServerSave { id, of, when } = asked;
        let save = self.tool_sign_ins.saves.begin(&id)?;
        let under_way = UnderWay { hub: Arc::clone(self), id, save };
        Ok(async move { under_way.hub.save_tool_server(of, when, &under_way.save, closed).await })
    }

    /// Lets go of a save of a tool server, as `ToolServerSaveCancel` says.
    pub(crate) fn tool_server_save_cancel(&self, id: &ToolServerSaveId) {
        self.tool_sign_ins.saves.cancel(id);
    }

    /// Saves `of` once it has listed its tools, or `when` says. Nothing is held while the server is asked, which ends as
    /// soon as the save is let go of, so its place among those asked is free at once.
    async fn save_tool_server(
        &self,
        of: ToolServerToCheck,
        when: SaveWhen,
        save: &Arc<Save>,
        mut closed: watch::Receiver<bool>,
    ) -> Result<ToolServerSaved, ApiError> {
        let when = match when {
            SaveWhen::Listed => When::Listed,
            SaveWhen::Anyway => When::Anyway,
            SaveWhen::Unknown => {
                return Err(ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know when that's to be saved"));
            }
        };
        let of = Arc::new(of);
        let (servers, saving, still) = (Arc::clone(&self.tool_servers), Arc::clone(&of), still_wanted(save, &closed));
        let to_ask = match off_thread(move || servers.save_or_ask(&saving, when, still)).await? {
            Saving::Done(kept) => return Ok(self.saved(kept)),
            Saving::Ask(to_ask) => to_ask,
        };
        let listed = {
            let _asking = self.tool_sign_ins.checks.begin()?;
            tokio::select! {
                listed = self.ask_for_tools(&to_ask) => listed,
                () = save.cancelled() => return Ok(ToolServerSaved::Cancelled),
                _ = closed.wait_for(|closed| *closed) => return Ok(ToolServerSaved::Cancelled),
            }
        };
        if let Err(why) = listed {
            return Ok(ToolServerSaved::NotListed { why });
        }
        let (servers, still) = (Arc::clone(&self.tool_servers), still_wanted(save, &closed));
        let kept = off_thread(move || servers.save_asked(&of, &to_ask, still)).await?;
        Ok(self.saved(kept))
    }

    /// What a save that wasn't refused answers, having told every client of a record it changed.
    fn saved(&self, kept: Kept) -> ToolServerSaved {
        match kept {
            Kept::Saved(change) => ToolServerSaved::Saved { server: self.announce_tool_server(change) },
            Kept::LetGo => ToolServerSaved::Cancelled,
        }
    }
}

/// A save the hub has under way, which it forgets however it ends, as when the hub stops first.
struct UnderWay {
    hub: Arc<Hub>,
    id: ToolServerSaveId,
    save: Arc<Save>,
}

impl Drop for UnderWay {
    fn drop(&mut self) {
        self.hub.tool_sign_ins.saves.end(&self.id, &self.save);
    }
}

/// Whether `save` is still wanted, which it isn't once it has been cancelled or `closed` says its connection has, and
/// has the hub begin to save it if it is.
fn still_wanted(save: &Arc<Save>, closed: &watch::Receiver<bool>) -> impl FnOnce() -> bool + Send + 'static {
    let (save, closed) = (Arc::clone(save), closed.clone());
    move || !*closed.borrow() && closed.has_changed().is_ok() && save.keep()
}
