//! What the transcript knows beyond the session's log, which every row is drawn with.

use super::pull_request::PullRequestStanding;
use super::record::AtWork;
use crate::attachments::Picture;
use crate::find::FindLook;
use crate::folds::Folds;
use crate::rows::SubagentCalls;
use client_core::session_log::RecordedApprovals;
use gpui_kit::SharedString;
use protocol::model::{AttachmentId, Seq, Subagent, SubagentId, ToolUseId};
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
    /// Each subagent's record, as the hub keeps them. None from a hub that doesn't.
    pub subagents: SubagentRecords,
    /// The `seq` of the event each subagent's call was drawn in, by that call, so the row is measured again when its
    /// record changes, which may come after the row was drawn. A call's row is measured only once it's been drawn, so
    /// one that isn't here needs nothing.
    pub subagent_rows: RefCell<HashMap<ToolUseId, Seq>>,
    /// In the subagent tab, the subagent it shows, alone.
    pub alone: Option<SubagentId>,
    /// What's being found in the chat, for rows to draw.
    pub find: Rc<FindLook>,
}

impl Model {
    /// Whether the agent's run says it records each subagent's work as its own, so the tab has it to show, as
    /// `Subagents` in `capability::background` says. Taken as so until its claims are known, as from a hub from
    /// before runs claimed, which only Claude's ran under.
    pub(crate) fn records_subagents(&self) -> bool {
        self.claims.as_ref().is_none_or(|run| run.claims.background.subagents.is_some())
    }

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
            subagents: SubagentRecords::default(),
            subagent_rows: RefCell::default(),
            alone: None,
            find: Rc::default(),
        }
    }
}

/// Each subagent's record, as the hub keeps them, by its ID and by the call that started it, which its row in the chat
/// is drawn for. Each is shared with client-core's snapshot, since every frame's rows take the records of the calls they
/// draw, and one can hold a call's whole input, such as all of a file it wrote.
#[derive(Clone, Default)]
pub(crate) struct SubagentRecords {
    by_id: HashMap<SubagentId, Arc<Subagent>>,
    started_by: HashMap<ToolUseId, SubagentId>,
}

impl SubagentRecords {
    pub(crate) fn get(&self, subagent: &SubagentId) -> Option<&Arc<Subagent>> {
        self.by_id.get(subagent)
    }

    /// The record of the subagent `call` started, once it's come.
    pub(crate) fn started_by(&self, call: &ToolUseId) -> Option<&Arc<Subagent>> {
        self.by_id.get(self.started_by.get(call)?)
    }

    /// The subagent `call` started, as its record names it. One whose record hasn't come is taken as named by its
    /// call, as a subagent from a hub from before subagents had IDs of their own is, which keeps no records.
    pub(crate) fn subagent_started_by(&self, call: &ToolUseId) -> SubagentId {
        self.started_by.get(call).cloned().unwrap_or_else(|| SubagentId::of_call(call))
    }

    /// Keeps `record` in place of the copy held of its subagent.
    pub(crate) fn insert(&mut self, record: Arc<Subagent>) {
        let id = record.id();
        self.started_by.insert(record.call.clone(), id.clone());
        self.by_id.insert(id, record);
    }
}

/// An idle chat, for tests that draw rows without one.
#[cfg(test)]
impl Default for Model {
    fn default() -> Self {
        Model::with(AtWork::default())
    }
}
