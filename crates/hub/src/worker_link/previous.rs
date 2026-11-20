//! Speaking protocol 4, the version before this one, to a worker of an older build, which the hub replaces with one of
//! its own once its workspace is idle. It goes with [`protocol::worker::previous`], at the next break, as
//! `docs/principles/protocol.md` says, and so does each place that asks [`WorkerLink::speaks_previous`]. So does
//! reading the forms from before an agent's calls, approvals and modes were typed from a worker, since only a worker of
//! protocol 4 can be of a build from before then.

use protocol::frame;
use protocol::model::{AgentKind, SessionId};
use protocol::worker::{WorkerFrame, previous};

use super::WorkerLink;
use crate::untyped::{Typing, naming_a_mode};

impl WorkerLink {
    /// Whether its worker chose protocol 4 in its hello.
    pub(crate) fn speaks_previous(&self) -> bool {
        self.version() == Some(previous::VERSION)
    }

    /// `bytes`, a frame from its worker, as this protocol reads it. From a worker of protocol 4, what its agents'
    /// events hold in the older forms is typed on `typing`, as a build from before they were typed sends them, a mode
    /// by the kind of the chat it's from, which `kind_of` finds, and the frames that protocol has of its own for what
    /// its agents report are read as the reports they'd be now.
    pub(crate) async fn read_frame(
        &self,
        bytes: &[u8],
        typing: &mut Typing,
        kind_of: impl AsyncFnOnce(SessionId) -> AgentKind,
    ) -> serde_json::Result<WorkerFrame> {
        if !self.speaks_previous() {
            return frame::decode(bytes);
        }
        let kind = match naming_a_mode(bytes) {
            Some(session) => kind_of(session).await,
            None => AgentKind::Unknown,
        };
        typing.frame(bytes, kind).or_else(|e| frame::decode::<previous::Frame>(bytes).map(WorkerFrame::from).or(Err(e)))
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::Report;
    use protocol::capability::calls::Call;
    use protocol::event::{ContentBlock, EventBody};
    use protocol::model::AgentKind;
    use protocol::worker::WorkerFrame;
    use serde_json::json;

    use super::WorkerLink;
    use crate::untyped::Typing;

    async fn read(worker: &WorkerLink, line: &[u8]) -> serde_json::Result<WorkerFrame> {
        worker.read_frame(line, &mut Typing::default(), async |_| AgentKind::Claude).await
    }

    /// Only a worker of protocol 4 sends its frames, so a worker of this protocol that sends one has sent what it
    /// shouldn't, which goes no further.
    #[tokio::test]
    async fn a_frame_of_protocol_4_is_read_as_the_frame_it_would_be_now_only_from_a_worker_of_protocol_4() {
        let line = br#"{"frame":"agent_context","session_id":"s","run":4,"context":{"used_tokens":5,"max_tokens":9}}"#;
        let refused = |_| None;

        let read_previous = read(&WorkerLink::answering_previous(refused), line).await;
        let Ok(WorkerFrame::AgentReport { report: Report::Context { context }, .. }) = read_previous else {
            panic!("{read_previous:?}")
        };
        assert_eq!((context.used_tokens, context.max_tokens), (5, 9));

        assert!(read(&WorkerLink::answering(refused), line).await.is_err(), "from a worker of this protocol");
    }

    /// Every build since a worker spoke protocol 4 types its agents' calls, so only a worker of protocol 4 has its
    /// calls typed as they're read, and a call in the older form from a worker of this protocol is one it doesn't know.
    #[tokio::test]
    async fn a_call_in_the_older_form_is_typed_only_from_a_worker_of_protocol_4() {
        let older = json!({"type":"tool_use","id":"c","name":"Bash","input":{"command":"ls"}});
        let line = json!({"frame":"agent_event","session_id":"s","run":1,"body":{"type":"assistant","blocks":[older]}});
        let blocks = async |worker: &WorkerLink| match read(worker, line.to_string().as_bytes()).await {
            Ok(WorkerFrame::AgentEvent { body: EventBody::Assistant { blocks, .. }, .. }) => blocks,
            read => panic!("{read:?}"),
        };
        let refused = |_| None;

        let typed = blocks(&WorkerLink::answering_previous(refused)).await;
        assert!(matches!(typed.as_slice(), [ContentBlock::Call { call: Call::Command(_), .. }]), "{typed:?}");
        assert_eq!(
            blocks(&WorkerLink::answering(refused)).await,
            [ContentBlock::Unknown],
            "from a worker of this protocol"
        );
    }
}
