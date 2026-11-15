//! What the transcript knows beyond the session's log, which every row is drawn with.

use super::pull_request::PullRequestStanding;
use super::record::AtWork;
use crate::attachments::Picture;
use crate::find::FindLook;
use crate::folds::Folds;
use crate::rows::SubagentCalls;
use client_core::session_log::RecordedApprovals;
use gpui_kit::SharedString;
use protocol::model::{AttachmentId, Seq, Subagent, SubagentId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub(crate) struct Model {
    /// What the agent the chat runs is called, as the hub describes it. `None` until it has, and from a hub that
    /// doesn't.
    pub agent: Option<SharedString>,
    pub claims: Option<protocol::model::RunCapabilities>,
    pub folds: Folds,
    /// The images prompts carry, by attachment, from the first time a row drew each.
    pub pictures: HashMap<AttachmentId, Picture>,
    /// Where the calls that started subagents are, as rows have looked for them.
    pub subagent_calls: RefCell<SubagentCalls>,
    /// What the agent is at, as the session's record says, which the transcript's last row draws.
    pub at_work: AtWork,
    /// The failed request whose card offers Retry, as [`crate::rows::retry_at`] says, as of the last look.
    pub retry_at: Option<Seq>,
    /// Where the workspace's pull request stands on GitHub, which a line for a wait on it says. `None` until it's
    /// known, and while there's none or nothing to say of it.
    pub pull_request: Option<PullRequestStanding>,
    /// The approvals the session's record says its agent is waiting on, which a plan's card and the panel follow.
    /// None from a hub that doesn't say, whose are found among what's loaded.
    pub approvals: Option<RecordedApprovals>,
    /// Each subagent's record, as the hub keeps them. None from a hub that doesn't. Each is shared with client-core's
    /// snapshot, since every frame's rows take the records of the calls they draw, and one can hold a call's whole
    /// input, such as all of a file it wrote.
    pub subagents: HashMap<SubagentId, Arc<Subagent>>,
    /// The `seq` of the event each subagent's call was drawn in, so the row is measured again when its record
    /// changes. A call's row is measured only once it's been drawn, so one that isn't here needs nothing.
    pub subagent_rows: RefCell<HashMap<SubagentId, Seq>>,
    /// In the subagent tab, the subagent it shows, alone.
    pub alone: Option<SubagentId>,
    /// What's being found in the chat, for rows to draw.
    pub find: Rc<FindLook>,
}

impl Model {
    /// One that's drawn nothing yet, for a chat or for its subagent tab.
    pub(super) fn with(at_work: AtWork) -> Self {
        Model {
            agent: None,
            claims: None,
            folds: Folds::default(),
            pictures: HashMap::new(),
            subagent_calls: RefCell::default(),
            at_work,
            retry_at: None,
            pull_request: None,
            approvals: None,
            subagents: HashMap::new(),
            subagent_rows: RefCell::default(),
            alone: None,
            find: Rc::default(),
        }
    }
}

/// An idle chat, for tests that draw rows without one.
#[cfg(test)]
impl Default for Model {
    fn default() -> Self {
        Model::with(AtWork::default())
    }
}
