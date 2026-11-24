//! A call that started a subagent: opened, it shows what the subagent was asked and its report, and none of its work,
//! which is in the subagent tab. The button at the end of its line, or of its line in the background while it works
//! there, opens the tab, which sits at the far left, after the file tab, where `Ctrl-Tab` comes to it, names its chat
//! in its band, and closes with `Cmd-W`, or with its chat. The chat holds its subagents' records until it closes, and
//! its outline counts only the agent's own events. Each needs the hub's records of the subagent, so these run on one,
//! but for a tab opened before its subagent's record comes, which only a fake hub with no record holds back.

#![cfg(test)]

use client_core::fake_hub::events::{finished_turn, first_turn, launched, subagent, user_prompt};
use desktop::workspace_pane::Showing;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::capability::calls::Call;
use protocol::capability::calls::call::Runs;
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{Record, SessionId, SessionStatus, SettledAt, Subagent, SubagentId};

use crate::menus::items;
use crate::support::{Gate, Stack, Ui, one_chat, open_on_the_chat};
use crate::tools::in_a_new_workspace;

/// What the fake agent says its subagent is for, and the description of the call the subagent makes.
const AGENT_LINE: &str = "Look around";
const SUBAGENT_CALL: &str = "List what's here";
/// The line of the agent's own call after the subagent, which runs something.
const COMMAND_LINE: &str = "Run";
/// What a subagent is for, too long for its line, so it gives way to the button there.
const LONG: &str = "Find every place in the crates that checks whether a signal shows Stop, and say what each one does";

pub(crate) fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

/// How many of the chat's subagents' records client-core holds.
fn records_held(ui: &Ui<'_>, chat: &SessionId) -> usize {
    ui.view().feed().unwrap().snapshot().subagents(chat).len()
}

/// The agent's call that started the subagent, once the chat has it.
fn agent_call(ui: &Ui<'_>) -> Option<protocol::model::ToolUseId> {
    ui.events().iter().find_map(|event| match &event.body {
        EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
            ContentBlock::Call { id, call: Call::Subagent(_) } => Some(id.clone()),
            _ => None,
        }),
        _ => None,
    })
}

/// Has the agent start a subagent for `what` and finish its turn. The ID of the button on its line that opens the
/// subagent tab, once it's there.
pub(crate) fn a_subagent_for(ui: &mut Ui<'_>, what: &str) -> SharedString {
    ui.paste_text(&format!("agent-calls:1:{what}; tool:echo after; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    let call = agent_call(ui).expect("the agent's call that started the subagent");
    let open = SharedString::from(format!("open-subagent/{call}"));
    // It's there once the app has the subagent's record, which comes on the feed.
    ui.wait("the button that opens the subagent tab", |ui| ui.has(open.clone()));
    open
}

fn bounds(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let id = id.into();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

/// A finished subagent's call, as its line, the button there that opens the subagent tab, and the tab: where it sits
/// among the others, its band, and how it closes, with `Cmd-W` and with its chat, which lets go of the subagent's
/// records.
#[gpui_kit::test]
fn a_finished_subagent_s_line_opens_its_tab_which_sits_after_the_file_tab_names_its_chat_and_closes_with_it(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let first = ui.active_chat();
    let open = the_button_on_a_finished_subagent_s_line(&mut ui);

    ui.click(open.clone());

    ui.wait("the subagent's work", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
    assert_eq!(showing(&ui), Showing::Subagent);
    assert!(ui.has("show-subagent-tab"));
    assert_eq!(items(&ui, "File")[5], "Close Subagent Tab", "⌘W closes the tab, not the chat behind it");
    let second = the_band_names_the_chat_and_shows_it(&mut ui, &first);
    the_tab_sits_after_the_file_tab_and_cmd_w_closes_it(&mut ui, &first, &second);
    // With the file tab, the subagent tab and the other chat's beside it, the first chat's close button can't be seen:
    // the window opens 1,320 px wide, which leaves the row of tabs 546 px beside both sidebars, and a tab is 200 px,
    // so the third is cut off before its close button, which scrolling the row would bring into view.
    ui.hover("show-file-tab");
    ui.click("close-file-tab");

    ui.click(open);
    ui.wait("the subagent tab, again", |ui| ui.has("show-subagent-tab"));
    ui.close_from_tab(&first);
    // Its tab goes with the chat's, so it's whether it's still open that says it closed.
    ui.wait("the subagent tab to close with its chat", |ui| {
        ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.subagent_tab().is_none())
    });
    assert_eq!(showing(&ui), Showing::Chat);
    // The app lets go of them once it has stopped following the chat, which comes after its tabs close.
    ui.wait("its subagents' records to go with it", |ui| records_held(ui, &first) == 0);
}

/// Has the agent start a subagent and finish its turn, and gives the ID of the button on its call's line that opens the
/// subagent tab, once it's shown: before the pointer is over the line, so the way to the tab is seen, apart from the
/// line, whose click opens the call where it is, to none of the subagent's work.
fn the_button_on_a_finished_subagent_s_line(ui: &mut Ui<'_>) -> SharedString {
    let open = a_subagent_for(ui, AGENT_LINE);

    ui.click_labelled(AGENT_LINE, 0);
    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "the subagent's call isn't in the chat");
    ui.click_labelled(AGENT_LINE, 0);
    ui.cx.update_window(ui.handle, |_, window, cx| window.hover("send", cx)).unwrap();
    ui.frame();

    let shown = ui.cx.update_window(ui.handle, |_, window, _| window.find(open.clone()).visible()).unwrap();
    assert!(shown, "the button shows with the call closed and the pointer elsewhere");
    open
}

/// Where the subagent tab sits doesn't say which chat it's from, so its band does, and that shows the chat when
/// clicked. Gives the other chat it opens to come to the tab from.
fn the_band_names_the_chat_and_shows_it(ui: &mut Ui<'_>, chat: &SessionId) -> SessionId {
    let other = ui.new_chat();
    // The last chat wraps around to it.
    ui.press("ctrl-tab");
    assert_eq!((showing(ui), ui.active_chat()), (Showing::Subagent, other.clone()), "from the other chat");
    // The chat may be titled while the band shows, so it's the title it has now.
    ui.wait("the band to name the chat whose subagent it shows", |ui| {
        let sessions = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.sessions().to_vec());
        let title = sessions.iter().find(|s| s.id == *chat).unwrap().title.clone();
        ui.shows_label("subagent-band-chat", title.as_deref().unwrap_or("Untitled"))
    });

    ui.click("subagent-band-chat");

    assert_eq!((showing(ui), ui.active_chat()), (Showing::Chat, chat.clone()), "the band shows its chat");
    other
}

/// The subagent tab sits at the far left, as the file tab does, just after it, so neither moves however many chats are
/// open, and `Ctrl-Tab` comes to them in that order. `Cmd-W` closes it, and leaves the chats open.
fn the_tab_sits_after_the_file_tab_and_cmd_w_closes_it(ui: &mut Ui<'_>, first: &SessionId, second: &SessionId) {
    let workspace = ui.view().selected().cloned().unwrap();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\n").unwrap();
    ui.wait("the file, in All files", |ui| ui.has("all-file-a.txt"));
    ui.click("all-file-a.txt");
    ui.wait("the file tab", |ui| showing(ui) == Showing::File);
    let left = |ui: &mut Ui<'_>, id: String| {
        ui.frame();
        bounds(ui, SharedString::from(id)).left()
    };

    let lefts = [
        left(ui, "show-file-tab".into()),
        left(ui, "show-subagent-tab".into()),
        left(ui, format!("chat-{first}")),
        left(ui, format!("chat-{second}")),
    ];
    assert!(lefts.is_sorted(), "the file tab, the subagent tab, then the chats, left to right: {lefts:?}");
    let at = |ui: &Ui<'_>| (showing(ui), ui.active_chat());
    ui.press("ctrl-tab");
    assert_eq!(showing(ui), Showing::Subagent, "just after the file tab");
    ui.press("ctrl-tab");
    assert_eq!(at(ui), (Showing::Chat, first.clone()), "the first chat after it");
    ui.press("ctrl-tab");
    assert_eq!(at(ui), (Showing::Chat, second.clone()));
    ui.press("ctrl-tab");
    assert_eq!(showing(ui), Showing::File, "the last chat wraps around to the file tab");
    ui.press("ctrl-shift-tab");
    assert_eq!(at(ui), (Showing::Chat, second.clone()), "and goes back");
    ui.show_chat(first);
    ui.press("ctrl-shift-tab");
    assert_eq!(showing(ui), Showing::Subagent, "just before the first chat");
    ui.press("secondary-w");

    assert_eq!(showing(ui), Showing::Chat, "Cmd-W closes the subagent tab");
    assert!(!ui.has("show-subagent-tab"));
    assert_eq!(ui.open_chats(), [first.clone(), second.clone()], "the chats stay open");
}

/// What a subagent is for gives way before the button to its tab does, so a long one ends in an ellipsis and the button
/// stays on its line. The subagent's events are in the chat's whole log, and not in its rows, so the outline's tick for
/// a prompt after them is at the row it's drawn in. The chat lets go of its subagents' records once its workspace is
/// archived.
#[gpui_kit::test]
fn a_long_title_gives_way_to_the_button_a_prompt_after_is_in_the_outline_at_its_row_and_archiving_lets_go(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_subagent_for(&mut ui, LONG);
    let line = ui.labelled(LONG)[0].bounds();
    let button = bounds(&mut ui, open);
    assert!(
        button.right() <= line.right(),
        "the button, to {:?}, is on its line, to {:?}",
        button.right(),
        line.right()
    );
    ui.click_labelled(LONG, 0);
    let chat = ui.active_chat();
    stack.run(&chat, "say:after it");
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

    // Before the row of its `seq`, since the subagent's events come before it in the whole log.
    let by_seq = usize::try_from(seq.get()).unwrap() - 1;
    assert!(row < by_seq, "row {row}, and {by_seq} by its seq");
    ui.wait("the outline", |ui| ui.has("outline"));
    assert!(ui.has(tick(row)), "its tick is at the row it's drawn in");
    let workspace = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.workspace().id.clone());
    assert_eq!(records_held(&ui, &chat), 1);
    // This test's hold on the chat's transcript, which would keep it, and what it follows, past its pane.
    drop(transcript);

    stack.client().workspace_archive(&workspace, false).unwrap();

    ui.wait("its pane to go", |ui| !ui.view().panes.contains_key(&workspace));
    // As each of its chats' transcripts goes, and with it what it followed of the chat's session.
    ui.wait("an archived workspace's chats to let go of their subagents' records", |ui| records_held(ui, &chat) == 0);
}

/// A tab opened on the subagent a call started, by that call, as one opened as soon as its call shows can be, shows the
/// subagent's work all the same, which the hub follows by the ID the agent gives it once the agent says it started. The
/// tab names it as the record does, and shows what it was asked. Here the record has nearly always come by the time the
/// tab opens, so the tab is named by it as it opens, and one named by its call until the record comes is the last
/// test's. It draws what its subagent is still writing, as a chat draws what its agent is, and then what it wrote, once
/// it's recorded.
#[gpui_kit::test]
fn a_subagent_tab_opened_by_its_call_names_it_as_its_record_does_and_draws_what_it_is_still_writing(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let chat = ui.active_chat();
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("agent-until:{done}:{AGENT_LINE}; say:done"));
    ui.press("enter");
    ui.wait("the agent's call that starts the subagent", |ui| agent_call(ui).is_some());
    let call = agent_call(&ui).unwrap();
    let pane = ui.pane().unwrap();
    let by_call = SubagentId::of_call(&call);
    ui.cx.update(|cx| pane.update(cx, |p, cx| p.open_subagent_tab(chat.clone(), by_call.clone(), "?".into(), cx)));

    let recorded = |ui: &Ui<'_>| {
        let view = ui.view();
        view.records().subagents(&chat).iter().find(|subagent| subagent.call == call).map(|subagent| subagent.id())
    };
    ui.wait("the subagent's record", |ui| recorded(ui).is_some());
    let id = recorded(&ui).unwrap();
    assert_ne!(id, by_call, "a subagent named by its call can't tell the two apart");
    ui.wait("the tab, named as the record names it", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.subagent_tab() == Some(&id))
    });
    ui.wait("what it was asked, from its record", |ui| ui.has(format!("prompt/call/{call}")));
    let tab = ui.cx.read_entity(&pane, |p, _| p.subagent_transcript()).unwrap();
    let drawn =
        |ui: &Ui<'_>, text: &str| ui.cx.read_entity(&tab, |t, _| t.kept_texts()).iter().any(|kept| kept == text);
    // The first piece of its report, which it holds the rest of until the gate opens.
    ui.wait("what it's still writing", |ui| drawn(ui, "There's "));
    done.open();

    ui.wait("its report, recorded", |ui| drawn(ui, "There's a README here."));
    ui.wait("the subagent's work, in its tab", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
}

/// A subagent at work in the background has a line at the chat's end, as all work there does, and the line ends with
/// the button its call's line has, so the way to its tab is at hand however far up the chat its call is. What it's
/// for gives way to the button, as on its call's line. Its call, opened while it works, makes room for its report.
#[gpui_kit::test]
fn a_working_subagent_s_line_opens_its_tab_and_its_call_opened_meanwhile_makes_room_for_its_report(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let chat = ui.active_chat();
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("agent-until:{done}:{LONG}; tool:echo after; say:done"));
    ui.press("enter");
    let named = || {
        let tasks = stack.client().session_get(&chat).unwrap().background_tasks;
        tasks.into_iter().find_map(|task| task.subagent.is_some().then_some(task.id))
    };
    ui.wait("its task to name its subagent", |_| named().is_some());
    let task = named().unwrap();
    let open = SharedString::from(format!("background-open/{task}"));
    // And its call's line, since the line in the background follows the session's record, which can come before the
    // call's row is taken in. The background line has the same label, so it's found as the only call.
    ui.wait("the button on its line, and its call's line", |ui| ui.has(open.clone()) && ui.shown("call/").len() == 1);

    let (button, words) = (bounds(&mut ui, open.clone()), bounds(&mut ui, format!("background-text/{task}")));
    let Some(ElementId::Name(its_call)) = ui.shown("call/").pop() else { panic!("its call's line has a name") };
    let its_call_line = bounds(&mut ui, its_call.clone());
    assert!(words.right() <= button.left(), "what it's for, to {:?}, ends before the button", words.right());
    assert!(button.right() <= its_call_line.right(), "the button, to {:?}, is in the column", button.right());
    // Opened while the subagent is still writing its report.
    ui.click(its_call);
    ui.wait("the call to open", |ui| ui.labelled(LONG).iter().any(|line| line.expanded() == Some(true)));
    ui.click(open);

    ui.wait("the subagent tab", |ui| showing(ui) == Showing::Subagent);
    ui.wait("the subagent's work, in its tab", |ui| ui.labelled(SUBAGENT_CALL).len() == 1);
    // Back in the chat, for the report to come to the call opened there.
    ui.show_chat(&chat);
    ui.wait("the chat", |ui| showing(ui) == Showing::Chat);
    done.open();
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));

    ui.wait("its report", |ui| ui.shown("report/").len() == 1);
    let report = ui.shown("report/").remove(0);
    let (report, after) = (bounds(&mut ui, report).bottom(), ui.labelled(COMMAND_LINE)[0].bounds().top());
    assert!(report <= after, "what came after is below the report: {report:?} {after:?}");
    assert!(ui.labelled(SUBAGENT_CALL).is_empty(), "its newest call shows only while it works");
}

/// A tab opened on a subagent by the call that started it, before the hub's record of the subagent has come, is named
/// by what the call says it's for, and once the record comes, as the record names it. The fake hub's chat has the call
/// and no record, and the record is then handed to client-core as a reply would hand it, which the feed tells of. It's
/// at a revision past the hub's, as one the feed sends is, so the session's records the tab's subscription lists, which
/// lack it, don't take it back, whenever they come.
#[gpui_kit::test]
fn a_subagent_tab_opened_before_its_record_comes_is_named_as_the_record_names_it_once_it_does(cx: &mut TestAppContext) {
    let call = "toolu-agent";
    let events = first_turn(
        user_prompt("Look around"),
        [
            subagent("msg-1", call, "Explore", AGENT_LINE, "Look around.", Runs::Background),
            launched(call),
            finished_turn(4, 0.01),
        ],
    );
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    let later = world.after();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let pane = ui.pane().unwrap();
    let by_call = SubagentId::of_call(&call.into());
    ui.cx.update_entity(&pane, |p, cx| p.open_subagent_tab(chat.clone(), by_call, AGENT_LINE.into(), cx));
    ui.wait("the tab, named by its call", |ui| !ui.labelled(&format!("Subagent: {AGENT_LINE}")).is_empty());
    let id = SubagentId::from("agent-a1b2");
    let recorded = "Look around the crates";
    let record = Subagent {
        session_id: chat.clone(),
        id: Some(id.clone()),
        description: Some(recorded.to_string()),
        rev: later,
        ..protocol::fixtures::subagent(call)
    };

    ui.view().feed().unwrap().take_record(Record::Subagent(record));

    ui.wait("the tab, named as the record names it", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.subagent_tab() == Some(&id))
    });
    assert!(!ui.labelled(&format!("Subagent: {recorded}")).is_empty(), "called what the record says it's for");
}
