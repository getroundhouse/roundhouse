//! Selecting in the subagent tab: what it says its subagent was asked, above its rows, and its report, below them,
//! with the rest. A long ask scrolling on under a drag from it is one of the cases in `super::areas`.

use desktop::workspace_pane::Showing;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, point, px};

use super::super::A_FRAME;
use super::kinds::{ASKED, REPORT, SUBAGENT_LINE};
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
    let open = a_subagent_for(&mut ui, SUBAGENT_LINE);
    open_its_tab(&mut ui, open);

    let asked = ui.shown("prompt/call/").remove(0);
    let asked = bounds(&mut ui, asked);
    click(&mut ui, point(asked.left() + px(20.), asked.top() + px(8.)), 1);
    ui.press("secondary-a");

    assert_eq!(copy(&mut ui), format!("{ASKED}\n\n{REPORT}"));
}

/// Opens a subagent's tab with the button `open` on its call's line, once what it was asked, and its report, are drawn
/// there.
pub(super) fn open_its_tab(ui: &mut Ui<'_>, open: impl Into<ElementId>) {
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    // Parsing a long text and measuring its row can move it out of view. Settle those frames before checking both
    // texts in one frame: separate `shown` calls can advance the app between finding the prompt and its report.
    ui.wait("what it was asked, and its report, laid out", |ui| {
        ui.settle(A_FRAME);
        ui.cx
            .update_window(ui.handle, |_, window, _| {
                let elements = window.elements();
                let drawn = |part: &str| {
                    elements.iter().any(|element| {
                        matches!(element.path().last(), Some(ElementId::Name(name)) if name.starts_with(part))
                            && element.bounds().size.height > px(0.)
                    })
                };
                drawn("prompt/call/") && drawn("report/call/")
            })
            .unwrap()
    });
}
