//! What the agent has heard that no request has taken in yet. The CLI says each task that ended with
//! `task_notification`, and the next request it sends the model takes it in, as `status` `requesting` says. One that
//! ends after a turn's last request starts a turn of its own once that turn ends, with `init` and no prompt, so what's
//! held as such a turn starts is what it took in.

use protocol::capability::turns::OwnTurn;
use protocol::event::{AgentTask, EndedTask, EventBody};
use serde_json::Value;

use super::super::tasks::{Tasks, outcome};
use super::super::{id_of, str_of};

/// What the agent has heard that it hasn't taken in, to say why it starts a turn by itself.
#[derive(Debug, Default)]
pub(in super::super) struct OwnTurns {
    /// The tasks heard to have ended since the model was last sent a request, in the order heard.
    held: Vec<EndedTask>,
    /// A prompt, or a message from another session, started since the last turn ended, so the next turn is its.
    prompted: bool,
}

impl OwnTurns {
    /// Notes what `value` says, with `tasks` as the lines before it left them. For the `init` of a turn the agent
    /// starts by itself, returns why, which goes in the log after it.
    pub(in super::super) fn note(&mut self, value: &Value, tasks: &Tasks) -> Option<EventBody> {
        match (str_of(value, "type"), str_of(value, "subtype")) {
            (Some("system"), Some("task_notification")) => self.held.extend(ended(value, tasks)),
            (Some("system"), Some("status")) if str_of(value, "status") == Some("requesting") => self.held.clear(),
            // A prompt the CLI starts on starts the next turn, whatever else it takes in, and so does a message from
            // another session. One taken mid-turn goes with that turn, which its end says.
            (Some("command_lifecycle"), _) if str_of(value, "state") == Some("started") => self.prompted = true,
            (Some("result"), _) => self.prompted = false,
            (Some("system"), Some("init")) => {
                let ended = std::mem::take(&mut self.held);
                let prompted = std::mem::take(&mut self.prompted);
                return (!prompted && !ended.is_empty()).then_some(EventBody::OwnTurn(OwnTurn::TaskEnded { ended }));
            }
            _ => {}
        }
        None
    }
}

/// The task a `task_notification` says ended, as the newest list said it, or `None` for one that isn't the agent's to
/// hear of. That's one no list held, which ran in the foreground, as a subagent's commands do, and was taken in by the
/// call it ran for, and one a subagent's call started, which the CLI tells that subagent of while it works. Once the
/// subagent has ended, the CLI tells the agent of few such ends, and of most nobody, so those aren't named either. The
/// CLI sends the shorter list after it.
fn ended(value: &Value, tasks: &Tasks) -> Option<EndedTask> {
    let id = id_of(value, "task_id")?;
    if tasks.a_subagents(&id) {
        return None;
    }
    let listed = tasks.task(&id)?;
    let call = id_of(value, "tool_use_id").or_else(|| listed.call.clone());
    Some(EndedTask { task: AgentTask { call, ..listed.clone() }, outcome: outcome(value), wait: None })
}
