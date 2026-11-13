//! Where a workspace stands, as its row shows it in the sidebar or in Home, and whether it needs a look, which a
//! collapsed project's row counts.

use gpui_kit::App;
use look::{Signal, Work};
use protocol::model::{SetupStatus, Workspace, WorkspaceStatus};

use super::signal;
use crate::app::AppView;

/// What a workspace's row shows of where it stands: its mark, by what its agents are doing and how its pull request
/// is getting on, and whether its name stands out.
pub(crate) struct Standing {
    pub(crate) signal: Signal,
    pub(crate) work: Work,
    /// Whether its name stands out, which it does while it [`needs_a_look`].
    pub(crate) needs_a_look: bool,
}

pub(crate) fn standing(app: &AppView, workspace: &Workspace, cx: &App) -> Standing {
    let chats = app.records().sessions(&workspace.id);
    let needs_a_look = needs_a_look(app, workspace, cx);
    let signal = if workspace.setup == SetupStatus::Running { Signal::SettingUp } else { signal::workspace(chats) };
    // The hub keeps it on the record, open or not. A hub too old to do that leaves it to the pane, if it's open.
    let work = crate::pull_request::recorded_work(workspace)
        .or_else(|| app.panes.get(&workspace.id).map(|pane| pane.read(cx).work()))
        .unwrap_or(Work::NoPr);
    Standing { signal, work, needs_a_look }
}

/// Whether a workspace needs a look: it has a reply nobody has read, or a chat waiting on an answer or an approval.
pub(super) fn needs_a_look(app: &AppView, workspace: &Workspace, cx: &App) -> bool {
    let unread = workspace.status != WorkspaceStatus::Archived && !app.unread_chats(&workspace.id, cx).is_empty();
    unread || signal::waiting_on_you(app.records().sessions(&workspace.id))
}

#[cfg(feature = "testing")]
impl AppView {
    /// What a workspace's mark shows of its pull request, in the sidebar or in Home.
    pub fn work_shown(&self, id: &protocol::model::WorkspaceId, cx: &App) -> Option<Work> {
        let workspace = self.records().workspace(id)?;
        Some(standing(self, workspace, cx).work)
    }
}
