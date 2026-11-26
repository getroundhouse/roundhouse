use gpui_kit::{EventEmitter, Window};
use protocol::model::WorkspaceId;

use super::WorkspacePane;

/// A pane asks its owning hub view to present window effects, so replies from an unselected hub cannot show a
/// modal over another hub. The callback keeps a weak pane handle, since its workspace may have gone meanwhile.
type Presentation = std::rc::Rc<dyn Fn(&mut Window, &mut gpui_kit::App)>;

#[derive(Clone)]
pub enum PaneEvent {
    Error(String),
    Archive(WorkspaceId, bool),
    /// Title the workspace again from what's been asked in it.
    RegenerateTitle(WorkspaceId),
    /// What's in front under the tabs changed, so the chat being read may have, and what has focus should follow it
    /// in the pane on screen.
    Showing,
    Present(Presentation),
    /// Show Git in Settings, where the default GitHub account is chosen and accounts are connected.
    OpenGit,
    /// Show another workspace, as the menu of a pull request a chat links to asks.
    GoToWorkspace(WorkspaceId),
}

impl EventEmitter<PaneEvent> for WorkspacePane {}
