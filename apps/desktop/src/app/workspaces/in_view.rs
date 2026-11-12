//! What's in view as a workspace is asked for, so that once it's made it shows only if the person hasn't gone
//! elsewhere meanwhile.

use protocol::model::WorkspaceId;

use crate::app::home::Main;

/// What the window shows: the selected workspace, or Home in its place, and whether Settings fills the window.
pub(super) struct InView {
    pub(super) selected: Option<WorkspaceId>,
    pub(super) main: Main,
    pub(super) settings: bool,
}

impl InView {
    /// Whether the person is where they were when this was in view, now that `now` is, so `made`, the workspace asked
    /// for then, can show in place of what they were looking at. Only where they are now counts, so going to another
    /// workspace and back is still being where they were, while another workspace, Home or Settings in view in its
    /// place, or the workspace in view in place of Home, is somewhere else. The first workspace is selected as it's
    /// listed, since there's nothing else, so its being selected isn't. The app can't tell a selection it made itself
    /// from one the person made, so the top of the sidebar selected in place of a workspace archived from elsewhere
    /// counts as going elsewhere too.
    pub(super) fn still(&self, now: &InView, made: &WorkspaceId) -> bool {
        let selected = now.selected == self.selected || now.selected.as_ref() == Some(made);
        selected && now.main == self.main && now.settings == self.settings
    }
}

#[cfg(test)]
mod tests {
    use super::{InView, Main};

    fn in_view(selected: Option<&str>, main: Main, settings: bool) -> InView {
        InView { selected: selected.map(Into::into), main, settings }
    }

    #[test]
    fn the_workspace_made_shows_only_where_the_person_hasnt_gone_elsewhere() {
        let made = "made".into();
        let asked = in_view(Some("alder"), Main::Workspace, false);

        assert!(asked.still(&asked, &made), "nothing has changed");
        assert!(!asked.still(&in_view(Some("birch"), Main::Workspace, false), &made), "another workspace");
        assert!(!asked.still(&in_view(Some("alder"), Main::Home, false), &made), "Home");
        assert!(!asked.still(&in_view(Some("alder"), Main::Workspace, true), &made), "Settings");
        let from_home = in_view(Some("alder"), Main::Home, false);
        assert!(from_home.still(&from_home, &made), "Home, where it was asked for");
        assert!(!from_home.still(&asked, &made), "back to the workspace from Home");
    }

    #[test]
    fn the_first_workspace_selected_as_its_listed_is_still_where_the_person_was() {
        let made = "made".into();
        let asked = in_view(None, Main::Home, false);

        assert!(asked.still(&in_view(Some("made"), Main::Home, false), &made));
        assert!(!asked.still(&in_view(Some("made"), Main::Workspace, false), &made), "unless they've left Home");
    }
}
