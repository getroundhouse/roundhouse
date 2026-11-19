//! Prompts the agent is sent once its turn ends, rather than into it: one its run refused while the turn ran, as one it
//! takes only after the turn, as Codex refuses a turn it can't steer, such as a review, and any recorded after one of
//! those, which has to follow it. Each stays untaken meanwhile, so the queue waits for it too, and goes to the agent
//! under the seq it was recorded at, as though it had just been sent. A turn that's stopped, fails or whose agent goes
//! drops them, as it does a prompt the agent hadn't taken yet, since the queue then waits for the person.

use super::super::{Command, LiveSession, LiveState, Origin};
use super::requests::Sent;
use protocol::attachment::AttachmentFile;
use protocol::capability::Control;
use protocol::capability::prompts::Extras;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::Seq;
use protocol::prompt::Prompt;
use tracing::warn;

/// A prompt recorded at `seq`, with its files in the worktree, as it's sent to the agent.
#[derive(Debug)]
pub(in super::super) struct Sendable {
    pub(super) seq: Seq,
    pub(super) prompt: Prompt<AttachmentFile>,
    pub(super) extras: Extras,
}

impl Sendable {
    fn control(&self) -> Control {
        Control::Prompt { seq: self.seq, prompt: self.prompt.clone(), extras: self.extras.clone() }
    }
}

impl LiveState {
    /// Whether a turn runs, as the session's record says.
    fn in_turn(&self) -> bool {
        self.progress.status.in_turn()
    }

    /// Whether a prompt is kept for the turn's end, which one recorded after it has to follow.
    fn keeps_any(&self) -> bool {
        !self.after_the_turn.is_empty()
    }
}

impl LiveSession {
    /// Sends the agent the prompt recorded at `prompt.seq`, or keeps it for the turn's end, behind any kept already, or
    /// if the run refuses it as one it takes only then.
    pub(super) async fn send_prompt(&self, prompt: Sendable) -> Result<(), ApiError> {
        match self.keep_for_the_turn_if(prompt, LiveState::keeps_any) {
            Some(prompt) => self.send_or_keep(prompt).await,
            None => Ok(()),
        }
    }

    /// Sends `prompt`, and keeps it for the turn's end if the run refuses it as one it takes only then.
    async fn send_or_keep(&self, mut prompt: Sendable) -> Result<(), ApiError> {
        // Twice, since the turn may have ended between the run refusing it and the hub taking that in.
        for _ in 0..2 {
            match self.send_to_agent(prompt.control()).await? {
                Sent::Took => return Ok(()),
                Sent::AfterTheTurn => match self.keep_for_the_turn_if(prompt, LiveState::in_turn) {
                    Some(again) => prompt = again,
                    None => return Ok(()),
                },
            }
        }
        Err(ApiError::new(ErrorCode::Worker, "the agent won't take the prompt, though its turn has ended"))
    }

    /// Keeps `prompt` for the turn's end if `keeps` says to, as the session's live state is, and otherwise gives it back
    /// to send now. Under the lock the turn's end is taken in under, so the turn can't end unseen between the two.
    fn keep_for_the_turn_if(&self, prompt: Sendable, keeps: impl FnOnce(&LiveState) -> bool) -> Option<Sendable> {
        let mut state = self.state.lock();
        if !keeps(&state) {
            return Some(prompt);
        }
        state.after_the_turn.push(prompt);
        None
    }

    /// Asks the command task to send what was kept for the turn's end, which has come, as [`Self::after_the_turn`]
    /// says.
    pub(in super::super) fn wake_after_the_turn(&self) {
        if let Err(e) = self.commands.try_send(Command::SendAfterTheTurn) {
            warn!(session = %self.id, "couldn't ask for what waited for the turn's end to be sent: {e}");
        }
    }

    /// Sends what was kept for the turn's end, in the order it was recorded, unless the turn was stopped or failed, or
    /// the chat has closed, which drops it. The turn may look to have started again by now, from a prompt recorded
    /// since, which is kept to follow these, so that isn't asked. Once the run refuses one again, the rest follow it.
    pub(super) async fn after_the_turn(&self) {
        let (kept, dropping) = {
            let mut state = self.state.lock();
            let kept = std::mem::take(&mut state.after_the_turn);
            // Any dropped since, as when the agent went away, which drops every prompt it hadn't taken.
            let untaken = state.progress.queue.untaken();
            let kept: Vec<Sendable> = kept.into_iter().filter(|prompt| untaken.contains(&prompt.seq)).collect();
            (kept, state.progress.queue.is_held() || !state.is_open())
        };
        if dropping {
            for prompt in kept {
                self.record_or_log(Origin::Hub, EventBody::PromptDropped { prompt: prompt.seq }).await;
            }
            return;
        }
        let mut kept = kept.into_iter();
        while let Some(prompt) = kept.next() {
            let seq = prompt.seq;
            match self.send_or_keep(prompt).await {
                Err(e) => self.not_delivered(seq, e).await,
                Ok(()) if self.state.lock().keeps_any() => {
                    self.state.lock().after_the_turn.extend(kept);
                    return;
                }
                Ok(()) => {}
            }
        }
    }
}
