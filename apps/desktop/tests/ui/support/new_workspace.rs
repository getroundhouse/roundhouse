//! Making workspaces through the new-workspace dialog.

#![cfg(test)]

use desktop::new_workspace::NewWorkspaceDialog;
use desktop::workspace_pane::Busy;
use gpui_kit::{AppContext as _, Entity, SharedString};
use protocol::model::WorkspaceId;

use super::Ui;

impl Ui<'_> {
    /// Opens the dialog with the `+` beside the first repository, as someone would.
    pub(crate) fn open_new_workspace(&mut self) {
        let id = format!("new-in-{}", self.first_repo_id());
        self.click(SharedString::from(id));
        self.wait("the new-workspace dialog", Ui::has_dialog);
    }

    /// Opens the dialog, and creates a workspace with nothing typed.
    pub(crate) fn new_workspace(&mut self) -> WorkspaceId {
        self.open_new_workspace();
        self.create_workspace()
    }

    /// Clicks Create in the dialog that's open, and waits until the new workspace is on screen.
    pub(crate) fn create_workspace(&mut self) -> WorkspaceId {
        let before = self.view().workspaces.len();
        self.click("new-workspace-create");
        self.wait_for_a_new_workspace(before)
    }

    /// Waits until there's one more workspace than `before`, with its chat on screen.
    pub(crate) fn wait_for_a_new_workspace(&mut self, before: usize) -> WorkspaceId {
        self.wait("the new workspace", |ui| {
            ui.view().workspaces.len() > before && ui.transcript().is_some() && ui.view().busy == Busy::Free
        });
        self.view().selected.clone().unwrap()
    }

    /// The dialog, once it's been opened.
    pub(crate) fn dialog(&self) -> Entity<NewWorkspaceDialog> {
        self.view().new_workspace.clone().expect("the dialog has been opened")
    }

    /// What's typed in the dialog.
    pub(crate) fn dialog_text(&self) -> String {
        self.cx.read_entity(&self.dialog(), NewWorkspaceDialog::prompt_text)
    }

    /// The names of the files waiting in the dialog, in order.
    pub(crate) fn dialog_attached(&self) -> Vec<String> {
        self.cx.read_entity(&self.dialog(), |d, _| d.attached_names())
    }
}
