//! Which workspace the window shows, and where it has been: those shown before it, and those Back has come back from,
//! for Back and Forward to go through, as a browser's do. One type holds both, so nothing can show a workspace without
//! the history knowing. The window keeps it until the app quits, as it keeps which sidebars show, since it's where this
//! window has been rather than anything about the workspaces. Only the workspace showing outlasts that, as a
//! preference.

use protocol::model::WorkspaceId;

/// The most it remembers each way, so a long day's history doesn't grow without end.
const KEPT: usize = 100;

#[derive(Debug, Default)]
pub(crate) struct Selection {
    showing: Option<WorkspaceId>,
    back: Vec<WorkspaceId>,
    forward: Vec<WorkspaceId>,
}

/// Which way through the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Way {
    Back,
    Forward,
}

impl Selection {
    /// The workspace selected, which shows unless Home or Settings is in its place.
    pub(crate) fn workspace(&self) -> Option<&WorkspaceId> {
        self.showing.as_ref()
    }

    /// Selects `to`, chosen however it was, so Back comes back to what showed before, and what Back had come back from
    /// is forgotten, as a browser forgets it once a link is followed. Choosing what's already selected changes nothing.
    pub(crate) fn choose(&mut self, to: WorkspaceId) {
        if self.showing.as_ref() == Some(&to) {
            return;
        }
        if let Some(from) = self.showing.replace(to) {
            push(&mut self.back, from);
        }
        self.forward.clear();
    }

    /// Selects `to` in place of a workspace that can't show any more, such as one archived from elsewhere. Nobody chose
    /// it, so Back doesn't come back from it.
    pub(crate) fn fall_back(&mut self, to: Option<WorkspaceId>) {
        self.showing = to;
    }

    /// Selects the nearest workspace `way` that `can_show`, if there's one, and says whether it did. Going the other
    /// way comes back to what showed. Those passed over are forgotten, as they can't be shown, such as one archived
    /// since.
    pub(crate) fn go(&mut self, way: Way, can_show: impl Fn(&WorkspaceId) -> bool) -> bool {
        let (from, to) = match way {
            Way::Back => (&mut self.back, &mut self.forward),
            Way::Forward => (&mut self.forward, &mut self.back),
        };
        while let Some(id) = from.pop() {
            if can_go_to(&id, self.showing.as_ref(), &can_show) {
                if let Some(left) = self.showing.replace(id) {
                    push(to, left);
                }
                return true;
            }
        }
        false
    }

    /// Whether [`Self::go`] would select a workspace, for the menus to grey out Back or Forward when it wouldn't.
    pub(crate) fn can_go(&self, way: Way, can_show: impl Fn(&WorkspaceId) -> bool) -> bool {
        let stack = match way {
            Way::Back => &self.back,
            Way::Forward => &self.forward,
        };
        stack.iter().any(|id| can_go_to(id, self.showing.as_ref(), &can_show))
    }
}

/// Whether going back or forward can come to `id`: it can show, and isn't what shows already, which a workspace passed
/// over can leave next in line.
fn can_go_to(id: &WorkspaceId, showing: Option<&WorkspaceId>, can_show: impl Fn(&WorkspaceId) -> bool) -> bool {
    Some(id) != showing && can_show(id)
}

/// Puts `id` on top of `stack`, unless it's there already, dropping the oldest past [`KEPT`].
fn push(stack: &mut Vec<WorkspaceId>, id: WorkspaceId) {
    if stack.last() == Some(&id) {
        return;
    }
    stack.push(id);
    if stack.len() > KEPT {
        stack.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> WorkspaceId {
        WorkspaceId::from(name)
    }

    /// A selection that has had each of `names` chosen in turn.
    fn chosen(names: &[&str]) -> Selection {
        let mut selection = Selection::default();
        for name in names {
            selection.choose(id(name));
        }
        selection
    }

    /// Goes `way`, with every workspace able to show, and says what shows then.
    fn went(selection: &mut Selection, way: Way) -> Option<WorkspaceId> {
        selection.go(way, |_| true).then(|| selection.workspace().cloned()).flatten()
    }

    #[test]
    fn back_and_forward_go_through_what_was_chosen_in_the_order_it_was() {
        let mut selection = chosen(&["a", "b", "c"]);
        assert!(!selection.can_go(Way::Forward, |_| true), "nothing to go forward to yet");

        for (way, to) in [(Way::Back, "b"), (Way::Back, "a"), (Way::Forward, "b"), (Way::Forward, "c")] {
            assert_eq!(went(&mut selection, way), Some(id(to)), "{way:?}");
        }
        assert_eq!(went(&mut selection, Way::Forward), None, "the end, rather than round to the start");
        assert_eq!(selection.workspace(), Some(&id("c")));
    }

    /// As a browser forgets the pages Back came back from once a link is followed, even with nothing selected, as
    /// after every workspace was archived.
    #[test]
    fn choosing_after_going_back_forgets_what_back_came_back_from() {
        let mut selection = chosen(&["a", "b", "c"]);
        went(&mut selection, Way::Back);

        selection.choose(id("d"));

        assert!(!selection.can_go(Way::Forward, |_| true), "c is forgotten");
        assert_eq!(went(&mut selection, Way::Back), Some(id("b")));
        went(&mut selection, Way::Back);
        selection.fall_back(None);
        selection.choose(id("e"));
        assert!(!selection.can_go(Way::Forward, |_| true), "with nothing selected too");
    }

    #[test]
    fn choosing_the_workspace_selected_isnt_a_step() {
        let mut selection = chosen(&["a", "b", "b"]);

        assert_eq!(went(&mut selection, Way::Back), Some(id("a")));
        assert_eq!(went(&mut selection, Way::Back), None);
    }

    /// Such as the one archived from elsewhere, whose place the window filled.
    #[test]
    fn falling_back_isnt_a_step() {
        let mut selection = chosen(&["a", "b"]);

        selection.fall_back(Some(id("c")));

        assert_eq!(went(&mut selection, Way::Back), Some(id("a")), "to a, as c wasn't chosen");
        assert!(selection.can_go(Way::Forward, |_| true), "and forward to c");
    }

    /// Such as one archived since. What's selected is passed over too, which a workspace passed over can leave next.
    #[test]
    fn what_cant_be_shown_is_passed_over_and_forgotten() {
        let mut selection = chosen(&["a", "b", "a", "c"]);
        let not_a = |id: &WorkspaceId| *id != WorkspaceId::from("a");

        assert!(selection.go(Way::Back, not_a));
        assert_eq!(selection.workspace(), Some(&id("b")), "past a");
        assert!(!selection.can_go(Way::Back, not_a), "nor the a before it");
        assert_eq!(went(&mut selection, Way::Forward), Some(id("c")), "and c is ahead");

        let mut selection = chosen(&["a", "b", "a"]);
        let not_b = |id: &WorkspaceId| *id != WorkspaceId::from("b");
        assert!(!selection.go(Way::Back, not_b), "past b is a, which is selected");
    }

    #[test]
    fn only_the_latest_are_kept() {
        let names: Vec<String> = (0..=KEPT + 1).map(|n| n.to_string()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut selection = chosen(&names);

        let mut steps = 0;
        while went(&mut selection, Way::Back).is_some() {
            steps += 1;
        }
        assert_eq!((steps, selection.workspace()), (KEPT, Some(&id("1"))), "the first is forgotten");
    }
}
