//! What a gallery screen is: what the hub holds, and the steps to take in the app before its picture is taken.

#![cfg(test)]

use desktop::updates::Kind;
use gpui_kit::Pixels;

use super::frame::Frame;
use super::world::World;

pub(crate) struct Screen {
    /// Also the image's file name.
    pub(crate) name: &'static str,
    /// The spec's file for the area it's about, in `docs/spec/`, which shows it beside the rules it goes with.
    pub(crate) spec: &'static str,
    /// What the hub holds. `None` is no hub at all.
    pub(crate) world: Option<fn() -> World>,
    /// What part of the window its picture shows.
    pub(crate) frame: Frame,
    pub(crate) steps: &'static [Step],
}

/// Something to do in the app once it has connected, each done once the one before it has finished.
pub(crate) enum Step {
    /// Selects the workspace with this name, and waits for its chat, its diff and its pull request to load.
    Select(&'static str),
    /// Clicks this file in the sidebar of changes, and waits for the file tab to show it.
    OpenFile(&'static str),
    /// Starts a comment on this line of the open file's new side, as the line's add button would.
    StartComment(u32),
    /// Shows All files in the sidebar of changes, and waits for it to list them.
    AllFiles,
    /// Clicks this file in All files, and waits for the file tab to show its lines, or its image.
    ViewFile(&'static str),
    /// Waits for the element with this ID to be drawn, such as what's in a folder that's read when it opens.
    WaitFor(&'static str),
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
    /// Presses the element with this ID and drags it this far down from where it was pressed, and holds it there, as
    /// partway through a drag.
    Drag(&'static str, Pixels),
    /// Types this where focus is, such as the composer.
    Type(&'static str),
    /// Presses these keys where focus is, as GPUI writes them, such as `secondary-f` for `Cmd-F` on a Mac.
    Press(&'static str),
    /// Rests the pointer on an element, and waits for the one with the ID `shows`, such as its tooltip.
    Hover {
        on: Target,
        shows: &'static str,
    },
    /// Pastes an image where focus is, and waits for the composer to take it.
    PasteImage(fn() -> Vec<u8>),
    /// Pastes a copied file with this name and text, and waits for the composer to take it.
    PasteFile(&'static str, &'static str),
    /// Presses Restart now in the update's card, and waits for what it asks.
    AskToFinish,
    /// Right-clicks something by its ID, which opens its menu.
    RightClick(&'static str),
    /// Picks the item with this label in the menu that's open.
    ClickItem(&'static str),
    /// Has the app show this version as downloaded, as if an update had come.
    UpdateReady(&'static str),
    /// Has the app show this problem updating, with this detail for when the pointer rests on it.
    UpdateProblem(Kind, &'static str),
    /// Has the app show a check for updates under way, as Check for Updates… starts one.
    UpdateChecking,
    /// Has the app show a check as having found nothing newer.
    UpToDate,
    /// Opens Settings, as its menu item does.
    OpenSettings,
    /// Clicks the transcript's line with this label, such as a tool call, to open it, has the chat go back to its end,
    /// and moves the pointer away, so the picture shows what it opened to rather than the line hovered. The first, if
    /// there are several.
    Open(&'static str),
    /// Opens the bar for finding text in the chat, as `Cmd-F` does, looks for this, and waits for every match to be
    /// counted and the first to be shown.
    Find(&'static str),
}

/// An element in the app, by its ID, or by the label it gives assistive apps, for one whose ID is made from data
/// the screen can't know, such as a tool call's line.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Id(&'static str),
    Label(&'static str),
}
