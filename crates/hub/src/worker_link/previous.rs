//! Speaking protocol 4, the version before this one, to a worker of an older build, which the hub replaces with one of
//! its own once its workspace is idle. It goes with [`protocol::worker::previous`], at the next break, as "Changing the
//! protocol" in `docs/architecture.md` says, and so does each place that asks [`WorkerLink::speaks_previous`].

use protocol::frame;
use protocol::worker::{WorkerFrame, previous};

use super::WorkerLink;

impl WorkerLink {
    /// Whether its worker chose protocol 4 in its hello.
    pub(crate) fn speaks_previous(&self) -> bool {
        self.version() == Some(previous::VERSION)
    }

    /// `bytes`, which this protocol has no form for, as the frame they'd be now, if its worker speaks protocol 4 and
    /// they're one of the frames that protocol has of its own.
    pub(crate) fn previous_frame(&self, bytes: &[u8]) -> Option<WorkerFrame> {
        if !self.speaks_previous() {
            return None;
        }
        frame::decode::<previous::Frame>(bytes).ok().map(WorkerFrame::from)
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::Report;
    use protocol::worker::WorkerFrame;

    use super::WorkerLink;

    /// Only a worker of protocol 4 sends its frames, so a worker of this protocol that sends one has sent what it
    /// shouldn't, which goes no further.
    #[tokio::test]
    async fn a_frame_of_protocol_4_is_read_as_the_frame_it_would_be_now_only_from_a_worker_of_protocol_4() {
        let line = br#"{"frame":"agent_context","session_id":"s","run":4,"context":{"used_tokens":5,"max_tokens":9}}"#;
        let refused = |_| None;

        let read = WorkerLink::answering_previous(refused).previous_frame(line);
        let Some(WorkerFrame::AgentReport { report: Report::Context { context }, .. }) = read else {
            panic!("{read:?}")
        };
        assert_eq!((context.used_tokens, context.max_tokens), (5, 9));

        assert_eq!(WorkerLink::answering(refused).previous_frame(line), None, "from a worker of this protocol");
    }
}
