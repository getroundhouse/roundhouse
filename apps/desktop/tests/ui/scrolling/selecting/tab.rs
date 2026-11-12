//! Selecting in the subagent tab: what it says its subagent was asked, above its rows, and its report, below them,
//! with the rest, and a long ask scrolling on under a drag from it.

use desktop::workspace_pane::Showing;
use gpui_kit::{TestAppContext, point, px};

use super::calls::{ASKED, REPORT, SUBAGENT_LINE, held_past, long};
use super::{bounds, click, copy};
use crate::subagents::{a_subagent_for, showing};
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// Cmd-A in the subagent tab copies what it was asked, then its report. Its rows have none of the transcript's texts
/// here, since its subagent's only words are its report's, so its last row is drawn empty.
#[gpui_kit::test]
fn what_the_subagent_tab_says_it_was_asked_and_reported_is_selected_with_the_rest(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    in_its_tab(&mut ui, SUBAGENT_LINE);

    let asked = ui.shown("prompt/call/").remove(0);
    let asked = bounds(&mut ui, asked);
    click(&mut ui, point(asked.left() + px(20.), asked.top() + px(8.)), 1);
    ui.press("secondary-a");

    assert_eq!(copy(&mut ui), format!("{ASKED}\n\n{REPORT}"));
}

/// A drag that starts in what the subagent tab says it was asked, at length, held below the area that scrolls it,
/// scrolls the area on to its end. It's long enough to be parsed on another thread, so it's drawn once its row has been
/// measured again with it.
#[gpui_kit::test]
fn a_drag_held_past_a_long_ask_in_the_subagent_tab_scrolls_its_area_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let asked = long().repeat(2);
    assert!(asked.len() > 4 * 1024, "parsed on another thread: {} bytes", asked.len());
    in_its_tab(&mut ui, &asked);

    held_past(&mut ui, "prompt/call/");
}

/// Has the agent start a subagent for `what`, and opens its tab once its report is drawn there.
fn in_its_tab(ui: &mut Ui<'_>, what: &str) {
    let open = a_subagent_for(ui, what);
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    // What it was asked is the first row's and its report the last's, so a frame can draw one before the other.
    ui.wait("what it was asked, and its report", |ui| {
        !ui.shown("prompt/call/").is_empty() && !ui.shown("report/call/").is_empty()
    });
}
