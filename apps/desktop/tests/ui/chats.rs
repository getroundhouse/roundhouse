//! Several chats in one workspace.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use client_core::fake_hub::events::{replied, taken, task, text, tool_result, tool_use, user_prompt};
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::event::{EventBody, TaskKind};
use protocol::model::{Seq, SessionId, SessionStatus, Title};
use serde_json::json;

use crate::support::{FakeHub, Gate, Stack, Ui};
use crate::tools::in_a_new_workspace;

/// Each `Cmd-T` makes a chat, even one pressed before the hub has answered for the one before it, and they open in the
/// order they were asked for, with the last showing.
#[gpui_kit::test]
fn a_new_workspace_starts_with_one_chat_and_the_keyboard_opens_moves_between_and_closes_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    // The first workspace is selected as soon as it exists, which is before its chat does.
    let workspace = ui.new_workspace();
    // Nothing says the app has done with opening chats for it, so this gives it a while to open a second.
    for _ in 0..30 {
        ui.frame();
        waiting::thread::for_what_mustnt_happen(Duration::from_millis(10));
    }
    assert_eq!(ui.open_chats().len(), 1, "a new workspace has one chat open");
    assert_eq!(client.session_list(&workspace).unwrap().len(), 1, "and one on the hub");
    let first = ui.active_chat();

    ui.press("secondary-t");
    ui.wait("a second chat", |ui| ui.open_chats().len() == 2);
    let second = ui.open_chats()[1].clone();
    ui.wait("the second to show", |ui| ui.active_chat() == second);
    ui.press("ctrl-tab");
    ui.wait("the first chat, wrapping around", |ui| ui.active_chat() == first);
    ui.press("ctrl-shift-tab");
    ui.wait("the second chat, wrapping back", |ui| ui.active_chat() == second);
    ui.press("secondary-w");
    ui.wait("the second chat to close", |ui| ui.open_chats() == [first.clone()]);
    ui.wait("the first chat to show", |ui| ui.active_chat() == first);

    // Wherever focus was, switching chats puts it back in the composer, ready to type.
    ui.press("secondary-t");
    ui.wait("a third chat", |ui| ui.open_chats().len() == 2);
    let third = ui.open_chats()[1].clone();
    ui.wait("the third to show", |ui| ui.active_chat() == third);
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.press("ctrl-tab");
    ui.wait("the first chat, from the third", |ui| ui.active_chat() == first);
    ui.frame();
    ui.input("ready");
    assert_eq!(ui.composer_text(), "ready");

    ui.press_at_once(&["secondary-t", "secondary-t"]);

    ui.wait("a chat for each of two pressed at once", |ui| ui.open_chats().len() == 4);
    let open = ui.open_chats();
    ui.wait("the last asked for to show", |ui| ui.active_chat() == open[3]);
    assert_eq!(open[..2], [first, third], "they open after those there");
    assert_eq!(ui.open_chats(), open, "and no more");
}

#[gpui_kit::test]
fn chats_keep_their_own_transcript_and_draft(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    ui.input("say:from the first chat");
    ui.press("enter");
    ui.wait_idle_with("the first answer", |text| text.contains("from the first"));

    let second = ui.new_chat();
    assert_ne!(first, second);
    assert_eq!(ui.composer_text(), "");
    assert_eq!(ui.assistant_text(), "", "a new chat starts empty");
    ui.input("say:from the second chat");
    ui.press("enter");
    ui.wait_idle_with("the second answer", |text| text.contains("from the second"));
    assert!(!ui.assistant_text().contains("from the first"));

    // Half-typed text stays with its chat, and comes back with it.
    ui.input("half typed");
    ui.show_chat(&first);
    ui.frame();
    assert_eq!(ui.composer_text(), "", "the draft belongs to the other chat");
    assert!(ui.assistant_text().contains("from the first") && !ui.assistant_text().contains("from the second"));
    ui.show_chat(&second);
    ui.frame();
    assert_eq!(ui.composer_text(), "half typed");
    ui.input(" more");
    assert_eq!(ui.composer_text(), "half typed more", "the composer has focus after switching");

    // The first prompt names the chat, everywhere.
    let titles: Vec<_> = client.session_list(&ui.view().selected.clone().unwrap()).unwrap();
    assert_eq!(titles[0].title.as_deref(), Some("say:from the first chat"));
    ui.wait("the title to reach the app", |ui| {
        let pane = ui.pane().unwrap();
        ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.title.clone()))
            == Title::tidied("say:from the second chat")
    });
}

#[gpui_kit::test]
fn a_closed_chat_can_be_reopened_and_the_last_one_is_replaced(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let workspace = ui.view().selected.clone().unwrap();
    let first = ui.active_chat();
    ui.input("say:keep this");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("keep this"));

    // Closing the only chat leaves a new, empty one rather than nothing.
    ui.close_from_tab(&first);
    ui.wait("a replacement", |ui| ui.open_chats().len() == 1 && ui.open_chats()[0] != first);
    let replacement = ui.open_chats()[0].clone();
    ui.wait("it to show", |ui| ui.active_chat() == replacement && ui.transcript().is_some());
    assert_eq!(ui.assistant_text(), "");
    assert!(client.session_get(&first).unwrap().closed_at_ms.is_some());

    // The history is still there, and the agent picks up where it left off.
    ui.pick("closed-chats", 0);
    ui.wait("it to come back", |ui| ui.active_chat() == first && ui.assistant_text().contains("keep this"));
    assert_eq!(ui.open_chats().len(), 2);
    ui.input("recall");
    ui.press("enter");
    ui.wait_idle_with("the agent to remember", |text| text.contains("I have seen: say:keep this"));
    assert_eq!(client.session_list(&workspace).unwrap().len(), 2);
}

#[gpui_kit::test]
fn closing_a_chat_nothing_was_sent_in_leaves_nothing_to_reopen(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let first = ui.active_chat();
    let second = ui.new_chat();
    let third = ui.new_chat();
    ui.show_chat(&second);

    ui.close_from_tab(&second);

    ui.wait("it to go", |ui| ui.open_chats() == [first.clone(), third.clone()]);
    ui.wait("the chat after it to show", |ui| ui.active_chat() == third);
    assert!(!ui.visible("closed-chats"), "there's nothing to reopen");
    let listed = || client.session_list(&workspace).unwrap().into_iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(listed(), [first.clone(), third.clone()]);

    // The last one goes too, and leaves a new one, as closing any last chat does.
    ui.close_from_tab(&first);
    ui.wait("the first to go", |ui| ui.open_chats() == [third.clone()]);
    ui.close_from_tab(&third);
    ui.wait("a new chat", |ui| ui.open_chats().len() == 1 && ui.open_chats()[0] != third);
    let replacement = ui.open_chats()[0].clone();
    ui.wait("it to show", |ui| ui.active_chat() == replacement);
    assert_eq!(listed(), [replacement]);
}

/// A tab's close button shows only under the pointer. Pressing it asks about that chat, without showing it, and
/// cancelling leaves the chat alone, working. Closing it then closes only that one, and the chat showing stays.
#[gpui_kit::test]
fn closing_a_chat_thats_working_asks_first_and_its_tabs_button_closes_only_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    // The workspace's first chat, so its tab and the next fit in the window, its long title and all.
    let working = ui.active_chat();
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("slow-until:{done}:still going"));
    ui.press("enter");
    ui.wait("it to be working", |ui| ui.session_status() == Some(SessionStatus::Running));
    let showing = ui.new_chat();
    let close = |chat: &SessionId| SharedString::from(format!("close-chat-{chat}"));
    ui.hover(SharedString::from(format!("chat-{showing}")));
    assert!(ui.visible(close(&showing)), "the pointer is over the new tab");
    ui.hover("send");
    assert!(!ui.visible(close(&working)) && !ui.visible(close(&showing)), "not even the chat showing has one");
    ui.hover(SharedString::from(format!("chat-{working}")));
    assert!(ui.visible(close(&working)), "the pointer is over its tab");
    assert!(!ui.visible(close(&showing)), "only the tab under the pointer has one");

    ui.click(close(&working));

    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    assert_eq!(ui.active_chat(), showing, "pressing a tab's close button doesn't also show its chat");
    ui.cx.simulate_prompt_answer("Cancel");
    ui.frame();
    assert_eq!(ui.open_chats(), [working.clone(), showing.clone()], "cancelling leaves it alone");
    let status =
        ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.sessions().iter().find(|s| s.id == working).map(|s| s.status));
    assert_eq!(status, Some(SessionStatus::Running), "and working");

    ui.close_from_tab(&working);

    ui.wait("the question again", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Close chat");
    ui.wait("it to close", |ui| ui.open_chats() == [showing.clone()]);
    assert_eq!(ui.active_chat(), showing);
}

/// Its turn is over, and stopping a turn wouldn't reach that work, so it takes a prompt, and closing it asks first.
#[gpui_kit::test]
fn whats_running_in_the_background_shows_until_it_finishes_and_closing_its_chat_meanwhile_asks(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    let done = stack.dir.path().join("done");
    ui.input(&format!("background:{}", done.display()));
    ui.press("enter");
    ui.wait("its turn to end with work left going", |ui| ui.session_status() == Some(SessionStatus::Background));
    let task = stack.client().session_get(&chat).unwrap().background_tasks[0].id.clone();
    let line = SharedString::from(format!("background-{task}"));

    ui.wait("its line", |ui| ui.has(line.clone()));
    assert!(ui.has("send") && !ui.has("stop"), "it takes a prompt");
    ui.close_from_tab(&chat);
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Cancel");
    ui.frame();
    assert_eq!(ui.open_chats(), std::slice::from_ref(&chat), "cancelling leaves it open");
    std::fs::write(&done, "").unwrap();

    ui.wait("the agent to pick up and finish", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && ui.assistant_text().contains("finished")
    });
    assert!(!ui.has(line));
}

/// A chat whose turn left a command and a monitor going in the background, by the calls that started them. What a
/// line says under the pointer is the app's, so a fake hub will do.
fn left_a_command_and_a_monitor_going() -> World {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let tests = json!({ "command": "cargo test", "description": "Run the tests", "run_in_background": true });
    let watch = json!({ "command": "tail -f test.log", "description": "Watch the run", "timeout_ms": 600_000 });
    let tasks =
        vec![task(TaskKind::Command, "toolu-1", "Run the tests"), task(TaskKind::Monitor, "toolu-2", "Watch the run")];
    let reply = "Both are going.";
    let events = vec![
        user_prompt("Run the tests, and watch the run"),
        taken(Seq::new(1)),
        tool_use("msg-1", "toolu-1", "Bash", tests),
        tool_result("toolu-1", "Command running in background."),
        tool_use("msg-1", "toolu-2", "Monitor", watch),
        tool_result("toolu-2", "Monitor started."),
        EventBody::AgentTasks { tasks, stopped: Vec::new() },
        text("msg-2", reply),
        replied(reply, 5, 0.01),
    ];
    world.chat(&workspace, SessionStatus::Background, events);
    world
}

/// A line is only what its task is for, so pointing at it says where it's running, and that a monitor is only
/// watching.
#[gpui_kit::test]
fn pointing_at_a_background_tasks_line_says_whether_its_running_or_watching(cx: &mut TestAppContext) {
    let fake = FakeHub::holding(left_a_command_and_a_monitor_going());
    let mut ui = Ui::open_on(cx, &fake);
    let line = |call: &str| SharedString::from(format!("background-task-{call}"));
    ui.wait("their lines", |ui| ui.has(line("toolu-1")) && ui.has(line("toolu-2")));
    assert!(!ui.has("background-pointed-at"));

    for (call, said) in [("toolu-1", "Running in the background"), ("toolu-2", "Watching in the background")] {
        // The pointer is put on the line each time round, since the line can move as the rest of the chat is drawn.
        ui.wait(said, |ui| {
            ui.hover(line(call));
            ui.wait_out_a_tooltip();
            ui.shows_label("background-pointed-at", said)
        });
    }
}

/// With more in the background than have a line each, the tasks of a kind share one line, which opens to their own
/// lines and closes again. It's at the end of a chat longer than the window, which keeps to its end, and it stays
/// where it is as it opens, so a second click with the pointer where it was closes it. Closed with the chat keeping
/// to its end again, which has taken the line up, it comes back down with the chat's end to the window's bottom.
/// Once its tasks have gone, the line that comes for the next lot starts closed, since it's another line.
#[gpui_kit::test]
fn several_background_tasks_of_a_kind_share_a_line_that_opens_to_theirs(cx: &mut TestAppContext) {
    const SEVERAL: &str = "several/tasks/commands";
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let chat = ui.active_chat();
    let going = || stack.client().session_get(&chat).unwrap().background_tasks;
    // Starts five commands that each run until its file is there, and gives their lines once the turn has left them.
    let start = |ui: &mut Ui<'_>, lot: &str| {
        let files = Vec::from_fn(5, |at| stack.dir.path().join(format!("{lot}-{at}")));
        let each: Vec<String> = files.iter().map(|file| format!("background:{}", file.display())).collect();
        // A click in the chat gives it the focus, for copying what's selected there.
        ui.click("composer");
        ui.input(&format!("burst:3000; {}", each.join("; ")));
        ui.press("enter");
        ui.wait(&format!("the {lot} turn to end with five left going"), |ui| {
            ui.session_status() == Some(SessionStatus::Background) && going().len() == 5
        });
        let lines: Vec<_> = going().iter().map(|task| SharedString::from(format!("background-{}", task.id))).collect();
        (files, lines)
    };
    let shown = |ui: &mut Ui<'_>, lines: &[SharedString]| lines.iter().filter(|line| ui.has((*line).clone())).count();

    let (files, lines) = start(&mut ui, "first");
    ui.wait("their one line", |ui| ui.shows_label(SEVERAL, "5 commands"));
    assert_eq!(shown(&mut ui, &lines), 0, "closed, none has a line of its own");
    // The line the turn ends with can come just after, and would move the tasks' line as it's clicked.
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));

    let pointer = ui.bounds(SEVERAL).center();
    ui.click_at(pointer);
    ui.wait("each task's own line", |ui| shown(ui, &lines) == 5);
    ui.click_at(pointer);
    ui.wait("their lines to go, at a click where the pointer was", |ui| shown(ui, &lines) == 0);

    // Open, with the chat keeping to its end again, the line is higher up, and closing it brings the chat's end back
    // down to the window's bottom, rather than leaving room under it that shows nothing.
    let closed_at_the_end = ui.bounds(SEVERAL);
    ui.click_at(pointer);
    ui.keep_to_the_end();
    ui.wait("each task's own line again", |ui| shown(ui, &lines) == 5);
    let higher_up = ui.bounds(SEVERAL);
    assert!(higher_up.top() < closed_at_the_end.top(), "the chat's end took the line up");
    ui.click_at(higher_up.center());
    ui.wait("their lines to go again", |ui| shown(ui, &lines) == 0);
    assert_eq!(ui.bounds(SEVERAL), closed_at_the_end, "the line came back down with the chat's end");
    ui.click_at(pointer);
    ui.wait("each task's own line, at a click where the line is", |ui| shown(ui, &lines) == 5);
    for file in files {
        std::fs::write(file, "").unwrap();
    }
    ui.wait("them all to finish", |ui| ui.session_status() == Some(SessionStatus::Idle) && going().is_empty());
    let (_, lines) = start(&mut ui, "second");

    ui.wait("the next lot's line", |ui| ui.shows_label(SEVERAL, "5 commands"));
    assert_eq!(shown(&mut ui, &lines), 0, "a line that came back starts closed");
}
