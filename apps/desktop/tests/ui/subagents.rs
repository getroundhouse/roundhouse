//! A call that started a subagent: opened, it shows what the subagent was asked and its report, and none of its work,
//! which is in the subagent tab. The button at the end of its line, or of its line in the background while it works
//! there, opens the tab, which sits at the far left, after the file tab, where `Ctrl-Tab` comes to it, names its chat
//! in its band, and closes with `Cmd-W`, or with its chat. The chat holds its subagents' records until it closes, and
//! its outline counts only the agent's own events.

#![cfg(test)]

use desktop::workspace_pane::Showing;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{SessionId, SettledAt, seq_index};

use crate::menus::items;
use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";
/// The line of the agent's own call after the subagent, which runs something.
const COMMAND_LINE: &str = "Run";

pub(crate) fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

/// How many of the chat's subagents' records client-core holds.
fn records_held(ui: &Ui<'_>, chat: &SessionId) -> usize {
    ui.view().feed().unwrap().read(|records| records.subagents().of(chat).len())
}

/// Has the agent start a subagent and finish its turn, and opens the subagent's call. The ID of the button on its line
/// that opens the subagent tab.
fn a_finished_subagent(ui: &mut Ui<'_>) -> SharedString {
    let open = a_subagent_for(ui, AGENT_LINE);
    ui.click_labelled(AGENT_LINE, 0);
    open
}

/// Has the agent start a subagent for `what` and finish its turn. The ID of the button on its line that opens the
/// subagent tab, once it's there.
pub(crate) fn a_subagent_for(ui: &mut Ui<'_>, what: &str) -> SharedString {
    ui.input(&format!("agent:{what}; tool:echo after; say:done"));
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
    let open = SharedString::from(format!("open-subagent/{call}"));
    // It's there once the subagent's record is, which comes on the feed.
    ui.wait("the button that opens the subagent tab", |ui| ui.has(open.clone()));
    open
}

#[gpui_kit::test]
fn a_subagent_s_call_shows_none_of_its_work_which_opens_in_the_subagent_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "the subagent's call isn't in the chat");

    ui.click(open);

    ui.wait("the subagent's work", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
    assert_eq!(showing(&ui), Showing::Subagent);
    assert!(ui.has("show-subagent-tab"));
    assert_eq!(items(&ui, "File")[4], "Close Subagent Tab", "⌘W closes the tab, not the chat behind it");
}

/// The button that opens the subagent tab shows on a closed call's line before the pointer is over it, so the way to
/// the tab is seen, apart from the line, whose click opens the call where it is.
#[gpui_kit::test]
fn the_button_to_the_subagent_tab_shows_on_a_closed_line(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    ui.click_labelled(AGENT_LINE, 0);
    ui.cx.update_window(ui.handle, |_, window, cx| window.hover("send", cx)).unwrap();
    ui.frame();

    let shown = ui.cx.update_window(ui.handle, |_, window, _| window.find(open.clone()).visible()).unwrap();
    assert!(shown, "the button shows with the call closed and the pointer elsewhere");
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
}

/// What the subagent is for gives way before the button does, so a long one ends in an ellipsis and the button stays
/// on its line.
#[gpui_kit::test]
fn the_button_to_the_subagent_tab_stays_on_a_line_with_a_long_title(cx: &mut TestAppContext) {
    const LONG: &str =
        "Find every place in the crates that checks whether a signal shows Stop, and say what each one does";
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_subagent_for(&mut ui, LONG);

    let line = ui.labelled(LONG)[0].bounds();
    let button = ui.cx.update_window(ui.handle, |_, window, _| window.find(open.clone()).bounds()).unwrap();

    assert!(
        button.right() <= line.right(),
        "the button, to {:?}, is on its line, to {:?}",
        button.right(),
        line.right()
    );
}

/// A subagent at work in the background has a line at the chat's end, as all work there does, and the line ends with
/// the button its call's line has, so the way to its tab is at hand however far up the chat its call is. What it's
/// for gives way to the button, as on its call's line.
#[gpui_kit::test]
fn a_subagent_s_line_in_the_background_opens_the_subagent_tab(cx: &mut TestAppContext) {
    const LONG: &str =
        "Find every place in the crates that checks whether a signal shows Stop, and say what each one does";
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let chat = ui.active_chat();
    let done = stack.dir.path().join("done");
    ui.input(&format!("agent-until:{}:{LONG}; say:done", done.display()));
    ui.press("enter");
    let named = || {
        let tasks = stack.client().session_get(&chat).unwrap().background_tasks;
        tasks.into_iter().find_map(|task| Some((task.id, task.call?)))
    };
    ui.wait("its task to name its call", |_| named().is_some());
    let (task, call) = named().unwrap();
    let open = SharedString::from(format!("background-open/{call}"));
    // And its call's line, since the line in the background follows the session's record, which can come before the
    // call's row is taken in.
    ui.wait("the button on its line, and its call's line", |ui| ui.has(open.clone()) && !ui.labelled(LONG).is_empty());

    let bounds = |ui: &mut Ui<'_>, id: SharedString| {
        ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
    };
    let (button, words) = (bounds(&mut ui, open.clone()), bounds(&mut ui, format!("background-text/{task}").into()));
    let its_call = ui.labelled(LONG)[0].bounds();
    assert!(words.right() <= button.left(), "what it's for, to {:?}, ends before the button", words.right());
    assert!(button.right() <= its_call.right(), "the button, to {:?}, is in the column", button.right());
    ui.click(open);

    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    std::fs::write(&done, "").unwrap();
    ui.wait("the subagent's work, in its tab", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
}

/// The subagent tab sits at the far left, as the file tab does, just after it, so neither moves however many chats are
/// open, and `Ctrl-Tab` comes to them in that order.
#[gpui_kit::test]
fn the_subagent_tab_sits_after_the_file_tab_before_the_chats_and_cmd_w_closes_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    let first = ui.active_chat();
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    let second = ui.new_chat();
    let workspace = ui.view().selected.clone().unwrap();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\n").unwrap();
    ui.wait("the file, in All files", |ui| ui.has("all-file-a.txt"));
    ui.click("all-file-a.txt");
    ui.wait("the file tab", |ui| showing(ui) == Showing::File);
    let left = |ui: &mut Ui<'_>, id: String| {
        ui.frame();
        ui.cx.update_window(ui.handle, |_, window, _| window.find(SharedString::from(id)).bounds().left()).unwrap()
    };

    let lefts = [
        left(&mut ui, "show-file-tab".into()),
        left(&mut ui, "show-subagent-tab".into()),
        left(&mut ui, format!("chat-{first}")),
        left(&mut ui, format!("chat-{second}")),
    ];
    assert!(lefts.is_sorted(), "the file tab, the subagent tab, then the chats, left to right: {lefts:?}");
    let at = |ui: &Ui<'_>| (showing(ui), ui.active_chat());
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::Subagent, "just after the file tab");
    ui.press("ctrl-tab");
    assert_eq!(at(&ui), (Showing::Chat, first.clone()), "the first chat after it");
    ui.press("ctrl-tab");
    assert_eq!(at(&ui), (Showing::Chat, second.clone()));
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::File, "the last chat wraps around to the file tab");
    ui.press("ctrl-shift-tab");
    assert_eq!(at(&ui), (Showing::Chat, second.clone()), "and goes back");
    ui.show_chat(&first);
    ui.press("ctrl-shift-tab");
    assert_eq!(showing(&ui), Showing::Subagent, "just before the first chat");
    ui.press("cmd-w");

    assert_eq!(showing(&ui), Showing::Chat);
    assert!(!ui.has("show-subagent-tab"));
    assert_eq!(ui.open_chats(), [first, second], "the chats stay open");
}

/// Where it sits doesn't say which chat it's from, so its band does, and that shows the chat when clicked.
#[gpui_kit::test]
fn the_subagent_tab_s_band_names_its_chat_which_it_shows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_finished_subagent(&mut ui);
    let chat = ui.active_chat();
    ui.click(open);
    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    let other = ui.new_chat();
    // The last chat wraps around to it.
    ui.press("ctrl-tab");
    assert_eq!((showing(&ui), ui.active_chat()), (Showing::Subagent, other), "from the other chat");
    // The chat may be titled while the band shows, so it's the title it has now.
    ui.wait("the band to name the chat whose subagent it shows", |ui| {
        let sessions = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.sessions.clone());
        let title = sessions.iter().find(|s| s.id == chat).unwrap().title.clone();
        ui.shows_label("subagent-band-chat", title.as_deref().unwrap_or("Untitled"))
    });

    ui.click("subagent-band-chat");

    assert_eq!((showing(&ui), ui.active_chat()), (Showing::Chat, chat));
}

#[gpui_kit::test]
fn the_subagent_tab_closes_with_its_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
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
    let mut ui = in_a_new_workspace(cx, &stack);
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
    let mut ui = in_a_new_workspace(cx, &stack);
    a_finished_subagent(&mut ui);
    stack.run(&ui.active_chat(), "say:after it");
    ui.wait_idle_with("the second turn", |text| text.contains("after it"));
    let prompts: Vec<_> =
        ui.events().into_iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).map(|e| e.seq).collect();
    let transcript = ui.transcript().unwrap();
    // Drawn where the agent took it.
    let took = |event: &protocol::event::SessionEvent| SettledAt::of(event).is_some_and(|(p, _)| p == prompts[1]);
    let row = ui.cx.read_entity(&transcript, |t, _| {
        (0..t.row_count()).find_map(|row| t.event_at(row).filter(took).map(|event| (row, event.seq)))
    });
    let (row, seq) = row.expect("the row that took the second prompt");
    let tick = |row: usize| SharedString::from(format!("outline-tick/{row}"));

    // The subagent's events are in the whole log before it, and not in the chat's rows.
    assert!(row < seq_index(seq) - 1, "row {row}, seq {seq}");
    ui.wait("the outline", |ui| ui.has("outline"));
    assert!(ui.has(tick(row)), "its tick is at the row it's drawn in");
}

#[gpui_kit::test]
fn a_subagent_s_call_opened_while_it_works_makes_room_for_its_report(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
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
