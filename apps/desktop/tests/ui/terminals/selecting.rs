//! Selecting what a terminal shows and copying it. What's selected and copied is the app's own, from client-core's
//! copy of what the terminal shows, so these open on a fake hub's terminal, whose screen never changes, with no hub,
//! worker or shell.

use client_core::fake_hub::World;
use desktop::workspace_pane::Showing;
use gpui_kit::component::input::SelectAll;
use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{AppContext as _, ClipboardItem, Pixels, Point, SharedString, TestAppContext, point, px};
use protocol::model::{SessionStatus, WorkspaceStatus};
use protocol::terminal::TerminalSize;

use super::{rows, showing};
use crate::support::{Cells, FakeHub, Ui, in_window};

/// What the made-up terminal shows: a command, what it printed, and the prompt after it.
const SHOWS: &[u8] = b"$ cat notes\r\ncopied\r\ndoubled word\r\neverything\r\n$ ";

/// The size the made-up terminal is, which a fake hub doesn't let the view change.
const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 8 };

/// What's on the clipboard before each way of copying, so what one copied doesn't count for the next.
const NOTHING_YET: &str = "nothing copied yet";

/// The app on a workspace with a chat, and a terminal a fake hub holds, in front, once it shows [`SHOWS`]. The hub
/// goes with the test, so it's returned to be kept.
pub(super) fn with_a_made_up_terminal(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let tab = SharedString::from(format!("terminal-{}", world.terminal(&workspace, SIZE, SHOWS)));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the terminal's tab", |ui| ui.has(tab.clone()));

    ui.click(tab);

    ui.wait("what the terminal shows", |ui| {
        ui.has(terminal_view::SHOWN_ID) && rows(ui).iter().any(|row| row.trim_end() == "everything")
    });
    (fake, ui)
}

/// The row of the terminal in front that says `text` and nothing else.
fn row_of(ui: &Ui<'_>, text: &str) -> u16 {
    u16::try_from(rows(ui).iter().position(|row| row.trim_end() == text).unwrap()).unwrap()
}

/// Where in the window `along` cells along row `row` of the terminal in front is.
fn place(ui: &mut Ui<'_>, along: f32, row: u16) -> Point<Pixels> {
    ui.frame();
    let terminal = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.terminal()).unwrap();
    let cell = ui.cx.read_entity(&terminal, |t, _| t.cell_size());
    ui.cx.update_window(ui.handle, |_, window, _| in_window(window, cell, Cells { along, row })).unwrap()
}

fn grid_origin(ui: &mut Ui<'_>) -> Point<Pixels> {
    ui.cx.update_window(ui.handle, |_, window, _| window.find(terminal_view::SHOWN_ID).bounds().origin).unwrap()
}

fn drag(ui: &mut Ui<'_>, from: Point<Pixels>, to: Point<Pixels>) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
}

fn clipboard(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_from_clipboard().and_then(|item| item.text())
}

const COPY: &str = if cfg!(target_os = "macos") { "cmd-c" } else { "ctrl-shift-c" };

/// Off the Mac, Ctrl-Shift-A, which archives the workspace elsewhere in the app, since a terminal there has it for
/// Select All, as it has Ctrl-Shift-C for Copy.
const SELECT_ALL: &str = if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-shift-a" };

/// A way of selecting what the terminal shows, and of copying it.
#[derive(Clone, Copy, Debug)]
enum Copying {
    /// From near the left of its first letter to near the right of its last, as a hand drags over a word, for the copy
    /// key. A cell at either end is taken in only once the drag covers the point six tenths of the way across it, as
    /// in Ghostty.
    ADragForTheCopyKey,
    /// A double click selects a word, which the Edit menu's Copy copies.
    ADoubleClick,
    /// The pane's Copy, which the window's Copy reaches once focus is on what lets it go on, as a text box with
    /// nothing selected does. The app gives the terminal focus back as soon as nothing has it, so it's asked directly.
    ThePanesCopyWhereverFocusIs,
    /// As the Edit menu's items do in a text box.
    TheEditMenusSelectAll,
    /// From the end of a word up past the top of the grid, over the tabs, which takes in everything from the start of
    /// the first row, as a drag goes on wherever the pointer goes once it has started on the grid.
    ADragPastTheGridsEdge,
    /// The terminal's Select All key, and its copy key.
    TheSelectAllKey,
}

impl Copying {
    fn select(self, ui: &mut Ui<'_>) {
        match self {
            Copying::ADragForTheCopyKey | Copying::ThePanesCopyWhereverFocusIs => {
                let row = row_of(ui, "copied");
                let (from, to) = (place(ui, 0.1, row), place(ui, 5.9, row));
                drag(ui, from, to);
            }
            Copying::ADoubleClick => {
                // With another word after it, so selecting the line would copy more.
                let on_it = place(ui, 3.5, row_of(ui, "doubled word")) - grid_origin(ui);
                let twice = ClickOptions::new().with_offset(on_it).with_count(2);
                let click = |_, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
                    window.click_with_options(terminal_view::SHOWN_ID, twice, cx);
                };
                ui.cx.update_window(ui.handle, click).unwrap();
            }
            Copying::TheEditMenusSelectAll => crate::menus::choose(ui, "Edit", "Select All"),
            Copying::ADragPastTheGridsEdge => {
                let from = place(ui, 5.9, row_of(ui, "copied"));
                let above = grid_origin(ui) - point(px(0.), px(30.));
                drag(ui, from, above);
            }
            Copying::TheSelectAllKey => ui.press(SELECT_ALL),
        }
    }

    fn copy(self, ui: &mut Ui<'_>) {
        match self {
            Copying::ADragForTheCopyKey | Copying::TheSelectAllKey => ui.press(COPY),
            Copying::ADoubleClick | Copying::TheEditMenusSelectAll | Copying::ADragPastTheGridsEdge => {
                crate::menus::choose(ui, "Edit", "Copy");
            }
            Copying::ThePanesCopyWhereverFocusIs => {
                let pane = ui.pane().unwrap();
                ui.cx
                    .update_window(ui.handle, |_, window, cx| pane.update(cx, |p, cx| p.copy_showing(window, cx)))
                    .unwrap();
            }
        }
    }

    /// Whether `copied` is what it selected, in a terminal whose first row says `first`.
    fn copied(self, copied: &str, first: &str) -> bool {
        match self {
            Copying::ADragForTheCopyKey | Copying::ThePanesCopyWhereverFocusIs => copied == "copied",
            Copying::ADoubleClick => copied == "doubled",
            Copying::TheEditMenusSelectAll | Copying::TheSelectAllKey => {
                copied.lines().any(|line| line == "everything")
            }
            Copying::ADragPastTheGridsEdge => copied.starts_with(first) && copied.ends_with("\ncopied"),
        }
    }
}

/// Each way in turn, in one window, in an order in which what the way before left selected isn't what this one
/// copies, so a way that selected nothing would copy what doesn't count.
#[gpui_kit::test]
fn what_is_selected_in_a_terminal_is_what_is_copied(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_made_up_terminal(cx);
    let first = rows(&ui)[0].trim_end().to_owned();
    let ways = [
        Copying::ADragForTheCopyKey,
        Copying::ADoubleClick,
        Copying::ThePanesCopyWhereverFocusIs,
        Copying::TheEditMenusSelectAll,
        Copying::ADragPastTheGridsEdge,
        Copying::TheSelectAllKey,
    ];

    for way in ways {
        ui.cx.write_to_clipboard(ClipboardItem::new_string(NOTHING_YET.to_owned()));

        way.select(&mut ui);
        way.copy(&mut ui);

        ui.wait(&format!("{way:?}: what was selected on the clipboard"), |ui| {
            clipboard(ui).is_some_and(|copied| way.copied(&copied, &first))
        });
    }
    let select_all = crate::menus::bound_where_focus_is(&mut ui, SELECT_ALL);
    assert!(select_all.is_some_and(|bound| bound.partial_eq(&SelectAll)), "{SELECT_ALL} is the terminal's");
    let archived = ui.view().records().workspaces().iter().any(|w| w.status != WorkspaceStatus::Ready);
    assert!(!archived && !ui.cx.has_pending_prompt(), "the workspace is left as it was");
    assert_eq!(showing(&ui), Showing::Terminal);
}
