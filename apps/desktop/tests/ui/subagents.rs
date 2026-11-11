//! A call that started a subagent: opened, it shows what the subagent was asked and its report, and none of its work,
//! which is in the subagent tab. The button at the end of its line opens the tab, which sits after its chat's tab,
//! where `Ctrl-Tab` comes to it, and closes with `Cmd-W`, or with its chat. The chat holds its subagents' records until
//! it closes, and its outline counts only the agent's own events.

#![cfg(test)]

use desktop::workspace_pane::Showing;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{SessionId, seq_index};

use crate::menus::items;
use crate::support::{Stack, Ui};
use crate::tools::in_full_access;

/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";
/// The line of the agent's own call after the subagent, which runs something.
const COMMAND_LINE: &str = "Run";

fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

/// How many of the chat's subagents' records client-core holds.
fn records_held(ui: &Ui<'_>, chat: &SessionId) -> usize {
    ui.view().feed().unwrap().read(|records| records.subagents().of(chat).len())
}

/// Has the agent start a subagent and finish its turn, and opens the subagent's call. The ID of the button on its line
/// that opens the subagent tab.
fn a_finished_subagent(ui: &mut Ui<'_>) -> SharedString {
    ui.input("agent:Look around; tool:echo after; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    let call = ui
        .events()
        .iter()
        .find_map(|event| match &event.body {
            EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
                ContentBlock::ToolUse { id, name, .. } if name == "Agent" => Some(id.clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("the agent's call that started the subagent");
    ui.click_labelled(AGENT_LINE, 0);
    let open = SharedString::from(format!("open-subagent/{call}"));
    // It's there once the subagent's record is, which comes on the feed.
    ui.wait("the button that opens the subagent tab", |ui| ui.has(open.clone()));
    open
}

#[gpui_kit::test]
fn a_subagent_s_call_shows_none_of_its_work_which_opens_in_the_subagent_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "the subagent's call isn't in the chat");

    ui.click(open);

    ui.wait("the subagent's work", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
    assert_eq!(showing(&ui), Showing::Subagent);
    assert!(ui.has("show-subagent-tab"));
    assert_eq!(items(&ui, "File")[4], "Close Subagent Tab", "⌘W closes the tab, not the chat behind it");
}

#[gpui_kit::test]
fn ctrl_tab_comes_to_the_subagent_tab_after_its_chat_and_cmd_w_closes_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    let first = ui.active_chat();
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    // A second chat after it, so the ring of tabs is the first chat, the subagent tab, then the second.
    let second = ui.new_chat();
    let at = |ui: &Ui<'_>| (showing(ui), ui.active_chat());

    ui.press("ctrl-shift-tab");
    assert_eq!(showing(&ui), Showing::Subagent, "just before the second chat");
    ui.press("ctrl-shift-tab");
    assert_eq!(at(&ui), (Showing::Chat, first.clone()), "just after the first");
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::Subagent);
    ui.press("ctrl-tab");
    assert_eq!(at(&ui), (Showing::Chat, second.clone()));
    ui.press("ctrl-shift-tab");
    ui.press("cmd-w");

    assert_eq!(showing(&ui), Showing::Chat);
    assert!(!ui.has("show-subagent-tab"));
    assert_eq!(ui.open_chats(), [first, second], "the chats stay open");
}

#[gpui_kit::test]
fn the_subagent_tab_closes_with_its_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    let chat = ui.active_chat();
    ui.click(open);
    ui.wait("the subagent tab", |ui| ui.has("show-subagent-tab"));

    ui.close_from_tab(&chat);

    // Its tab goes with the chat's, so it's whether it's still open that says it closed.
    ui.wait("the subagent tab to close", |ui| {
        ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.subagent_tab().is_none())
    });
    assert_eq!(showing(&ui), Showing::Chat);
    assert_eq!(records_held(&ui, &chat), 0, "and its subagents' records go with it");
}

#[gpui_kit::test]
fn an_archived_workspace_s_chats_let_go_of_their_subagents_records(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    a_finished_subagent(&mut ui);
    let chat = ui.active_chat();
    let workspace = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.workspace.id.clone());
    assert_eq!(records_held(&ui, &chat), 1);

    stack.client().workspace_archive(&workspace, false).unwrap();

    ui.wait("its pane to go", |ui| !ui.view().panes.contains_key(&workspace));
    assert_eq!(records_held(&ui, &chat), 0);
}

#[gpui_kit::test]
fn a_prompt_after_a_subagent_s_work_is_in_the_outline_at_its_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    a_finished_subagent(&mut ui);
    stack.run(&ui.active_chat(), "say:after it");
    ui.wait_idle_with("the second turn", |text| text.contains("after it"));
    let prompts: Vec<_> =
        ui.events().into_iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).map(|e| e.seq).collect();
    let transcript = ui.transcript().unwrap();
    let row = ui.cx.read_entity(&transcript, |t, _| {
        (0..t.row_count()).find(|row| t.event_at(*row).is_some_and(|event| event.seq == prompts[1]))
    });
    let row = row.expect("the second prompt's row");
    let tick = |row: usize| SharedString::from(format!("outline-tick/{row}"));

    // The subagent's events are in the whole log before it, and not in the chat's rows.
    assert!(row < seq_index(prompts[1]) - 1, "row {row}, seq {}", prompts[1]);
    ui.wait("the outline", |ui| ui.has("outline"));
    assert!(ui.has(tick(row)), "its tick is at the row it's drawn in");
}

#[gpui_kit::test]
fn a_subagent_s_call_opened_while_it_works_makes_room_for_its_report(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
    ui.input("agent:Look around; tool:echo after; say:done");
    ui.press("enter");
    // The subagent works for a second before its call and its report, so its call is open, and measured, first.
    ui.wait("the subagent's call", |ui| ui.labelled(AGENT_LINE).len() == 1);
    ui.click_labelled(AGENT_LINE, 0);
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));

    ui.wait("its report", |ui| ui.shown("report/").len() == 1);
    let report = ui.shown("report/").remove(0);
    let (report, after) = (bottom(&mut ui, report), ui.labelled(COMMAND_LINE)[0].bounds().top());
    assert!(report <= after, "what came after is below the report: {report:?} {after:?}");
    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "its newest call shows only while it works");
}

fn bottom(ui: &mut Ui<'_>, id: ElementId) -> gpui_kit::Pixels {
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds().bottom()).unwrap()
}
