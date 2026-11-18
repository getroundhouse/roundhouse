use protocol::attachment::NewAttachment;
use protocol::capability::Report;
use protocol::event::{EventBody, ExitReason};
use protocol::tail::{Retrying, StreamDelta};

#[derive(Debug)]
pub enum AgentOutput {
    Event {
        body: EventBody,
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep. `raw` is without their bytes.
        attachments: Vec<NewAttachment>,
    },
    Delta(StreamDelta),
    /// The agent is trying a failed request to its model again. Only the newest matters, and only until the agent
    /// writes again, so it's live, as a delta is.
    Retrying(Retrying),
    /// What the agent says that isn't an event, which says who keeps it, such as how full its context is now, one of
    /// its own requests to its model, or what the adapter's own asking found of the account's limits.
    Report(Report),
    /// Always the last thing sent.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
        stderr_tail: Vec<String>,
        requested: bool,
    },
}

impl AgentOutput {
    /// An event that names no images.
    pub fn event(body: EventBody, raw: Option<String>) -> Self {
        AgentOutput::Event { body, raw, attachments: Vec::new() }
    }

    /// How the exit [`AgentOutput::Exited`] reports is recorded.
    pub fn exit_event(code: Option<i32>, signal: Option<i32>, stderr_tail: Vec<String>, requested: bool) -> EventBody {
        let reason = if requested { ExitReason::Requested } else { ExitReason::Unexpected };
        EventBody::AgentExited { reason, code, signal, stderr_tail }
    }

    /// What's recorded of it: the event as it is, and the exit as `AgentExited`, or nothing, for what isn't an event.
    pub fn recorded(&self) -> Option<EventBody> {
        match self {
            AgentOutput::Event { body, .. } => Some(body.clone()),
            AgentOutput::Exited { code, signal, stderr_tail, requested } => {
                Some(Self::exit_event(*code, *signal, stderr_tail.clone(), *requested))
            }
            AgentOutput::Delta(_) | AgentOutput::Retrying(_) | AgentOutput::Report(_) => None,
        }
    }
}
