//! What a gallery screen is: what the hub holds, and the steps to take in the app before its picture is taken.

#![cfg(test)]

use super::world::World;

pub(crate) struct Screen {
    /// Also the image's file name.
    pub(crate) name: &'static str,
    /// The spec's file for the area it's about, in `docs/spec/`, which shows it beside the rules it goes with.
    pub(crate) spec: &'static str,
    /// What the hub holds. `None` is no hub at all.
    pub(crate) world: Option<fn() -> World>,
    pub(crate) steps: &'static [Step],
}

/// Something to do in the app once it has connected, each done once the one before it has finished.
pub(crate) enum Step {
    /// Selects the workspace with this name, and waits for its chat, its diff and its pull request to load.
    Select(&'static str),
    /// Clicks this file in the sidebar of changes, and waits for the diff tab to show it.
    OpenFile(&'static str),
    /// Starts a comment on this line of the open file's new side, as the line's add button would.
    StartComment(usize),
    OpenSetupLog,
    /// Presses a button, such as one that opens a menu.
    Click(&'static str),
    /// Moves the pointer somewhere with no hover of its own, so the picture shows what's open rather than what's
    /// under the pointer.
    MoveAway,
    /// Moves the pointer over the tab of the open chat at this position in the workspace showing, and leaves it there.
    HoverTab(usize),
    /// Moves the pointer over the element with this ID, and leaves it there, for what shows only under the pointer.
    Point(&'static str),
    /// Types this where focus is, such as the composer.
    Type(&'static str),
    /// Rests the pointer on an element, and waits for the one with the ID `shows`, such as its tooltip.
    Hover {
        on: Target,
        shows: &'static str,
    },
    /// Pastes an image where focus is, and waits for the composer to take it.
    PasteImage(fn() -> Vec<u8>),
    /// Pastes a copied file with this name and text, and waits for the composer to take it.
    PasteFile(&'static str, &'static str),
    /// Presses Finish now in the update's card, and waits for what it asks.
    AskToFinish,
    /// Right-clicks something by its ID, which opens its menu.
    RightClick(&'static str),
    /// Picks the item with this label in the menu that's open.
    ClickItem(&'static str),
    /// Has the app show this version as downloaded, as if an update had come.
    UpdateReady(&'static str),
    /// Has the app show that it couldn't reach the update server.
    UpdateProblem,
    /// Opens Settings, as its menu item does.
    OpenSettings,
    /// Clicks the transcript's line with this label, such as a tool call, to open it, then moves the pointer away,
    /// so the picture shows it open rather than hovered. The first, if there are several.
    Open(&'static str),
    /// Waits for the transcript's scrollbar to fade, for a screen whose chat scrolled. It shows for two seconds after a
    /// scroll by the wall clock, which the app's clock in the gallery doesn't move, so otherwise the picture would
    /// depend on how fast it was taken.
    LetTheScrollbarFade,
}

/// An element in the app, by its ID, or by the label it gives assistive apps, for one whose ID is made from data
/// the screen can't know, such as a tool call's line.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Id(&'static str),
    Label(&'static str),
}
