use protocol::attachment::NewAttachment;
use protocol::event::EventBody;
use protocol::model::{CacheRequest, ContextUsage, UsageLimit};
use protocol::tail::StreamDelta;

#[derive(Debug)]
pub enum AgentOutput {
    Event {
        body: EventBody,
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep. `raw` is without their bytes.
        attachments: Vec<NewAttachment>,
    },
    Delta(StreamDelta),
    /// How full the agent's context is now. It answers the adapter's own question rather than being something the
    /// agent said, and only the newest matters, so it isn't an event.
    Context(ContextUsage),
    /// One of the agent's own requests to its model, as the model started to answer it. It isn't an event, since a
    /// turn makes many, but each matters, since the hub compares each with the one before.
    Cache(CacheRequest),
    /// What's used of each of the account's limits now, shortest window first, when the adapter asked rather than
    /// the agent saying so by itself, and whether that's every model's own week the account has, as an answer that
    /// lists them all is. It answers the adapter's own question, so it isn't an event, as `Context` isn't.
    Limits {
        limits: Vec<UsageLimit>,
        every_models_week: bool,
    },
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
}
