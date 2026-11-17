//! Selecting in the subagent tab: what it says its subagent was asked, above its rows, and its report, below them,
//! with the rest, and a long ask scrolling on under a drag from it. Only what the hub recorded of the subagent matters,
//! so each chat is made up for a fake hub, as the hub records the fake agent's subagent.

use client_core::fake_hub::events::{text, within};
use desktop::workspace_pane::Showing;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext, point, px};
use protocol::event::EventBody;

use super::super::{A_FRAME, turns};
use super::areas::{held_past, long};
use super::kinds::{ASKED, REPORT, SUBAGENT_LINE};
use super::made_up::{open_on_chats_of, subagent};
use super::{LAST, bounds, click, copy};
use crate::subagents::showing;
use crate::support::{FakeHub, Ui};

/// Cmd-A in the subagent tab copies what it was asked, then its report. Its rows have none of the transcript's texts
/// here, since its subagent's only words are its report's, so its last row is drawn empty.
#[gpui_kit::test]
fn what_the_subagent_tab_says_it_was_asked_and_reported_is_selected_with_the_rest(cx: &mut TestAppContext) {
    let (_fake, mut ui) = in_its_tab(cx, ASKED);

    let asked = ui.shown("prompt/call/").remove(0);
    let asked = bounds(&mut ui, asked);
    click(&mut ui, point(asked.left() + px(20.), asked.top() + px(8.)), 1);
    ui.press("secondary-a");

    assert_eq!(copy(&mut ui), format!("{ASKED}\n\n{REPORT}"));
}

/// A drag that starts in what the subagent tab says it was asked, at length, held below the area that scrolls it,
/// scrolls the area on to its end, as one in a row's area does in `super::areas`. It's long enough to be parsed on
/// another thread, so it's drawn once its row has been measured again with it.
#[gpui_kit::test]
fn a_drag_held_past_a_long_ask_in_the_subagent_tab_scrolls_its_area_on(cx: &mut TestAppContext) {
    let asked = long(5);
    assert!(asked.len() > 4 * 1024, "parsed on another thread: {} bytes", asked.len());
    let (_fake, mut ui) = in_its_tab(cx, &asked);

    held_past(&mut ui, "a long ask in the subagent tab", "prompt/call/");
}

/// The window on a fake hub holding a chat whose agent started a subagent, asked `asked`, whose only words were its
/// report, [`REPORT`], as the fake agent's subagent's are, and then said [`LAST`], with the subagent's tab open from
/// the button on its call's line. The hub goes with the test, so it's returned to be kept.
fn in_its_tab<'a>(cx: &'a mut TestAppContext, asked: &str) -> (FakeHub, Ui<'a>) {
    // The call `subagent` makes, as the first.
    let call = "toolu-agent-1";
    let mut did = subagent(1, SUBAGENT_LINE, asked, REPORT);
    let ended = did.iter().position(|event| matches!(event, EventBody::SubagentEnded { .. })).unwrap();
    did.insert(ended, within(call, text("msg-subagent", REPORT)));
    did.push(text("msg-last", LAST));
    let chat = turns(vec![(format!("agent-calls:1:{SUBAGENT_LINE}; say:{LAST}"), did)]);
    let (fake, mut ui, _, _) = open_on_chats_of(cx, vec![chat]);
    open_its_tab(&mut ui, SharedString::from(format!("open-subagent/{call}")));
    (fake, ui)
}

/// Opens a subagent's tab with the button `open` on its call's line, once what it was asked, and its report, are drawn
/// there.
fn open_its_tab(ui: &mut Ui<'_>, open: impl Into<ElementId>) {
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
