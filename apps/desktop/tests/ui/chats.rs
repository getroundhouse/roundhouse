//! Several chats in one workspace.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::{SessionId, SessionStatus};

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_new_workspace_starts_with_one_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    // The first workspace is selected as soon as it exists, which is before its chat does.
    let workspace = ui.new_workspace();
    for _ in 0..30 {
        ui.frame();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ui.open_chats().len(), 1);
    assert_eq!(client.session_list(&workspace).unwrap().len(), 1);
}

#[gpui_kit::test]
fn chats_keep_their_own_transcript_and_draft(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
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
            == Some("say:from the second chat".to_string())
    });
}

#[gpui_kit::test]
fn the_keyboard_opens_moves_between_and_closes_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    ui.press("cmd-t");
    ui.wait("a second chat", |ui| ui.open_chats().len() == 2);
    let second = ui.open_chats()[1].clone();
    ui.wait("it to show", |ui| ui.active_chat() == second);
    ui.press("ctrl-tab");
    ui.wait("the first chat, wrapping around", |ui| ui.active_chat() == first);
    ui.press("ctrl-shift-tab");
    ui.wait("the second chat, wrapping back", |ui| ui.active_chat() == second);
    ui.press("cmd-w");
    ui.wait("the second chat to close", |ui| ui.open_chats() == [first.clone()]);
    ui.wait("the first chat to show", |ui| ui.active_chat() == first);

    // Wherever focus was, switching chats puts it back in the composer, ready to type.
    ui.press("cmd-t");
    ui.wait("another chat", |ui| ui.open_chats().len() == 2);
    let third = ui.open_chats()[1].clone();
    ui.wait("it to show", |ui| ui.active_chat() == third);
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.press("ctrl-tab");
    ui.wait("the first chat", |ui| ui.active_chat() == first);
    ui.frame();
    ui.input("ready");
    assert_eq!(ui.composer_text(), "ready");
}

#[gpui_kit::test]
fn a_closed_chat_can_be_reopened_and_the_last_one_is_replaced(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
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
fn closing_a_chat_thats_working_asks_first(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    let working = ui.new_chat();
    ui.input("slow:200:still going");
    ui.press("enter");
    ui.wait("it to be working", |ui| ui.session_status() == Some(SessionStatus::Running));

    ui.close_from_tab(&working);
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Cancel");
    ui.frame();
    assert_eq!(ui.open_chats().len(), 2, "cancelling leaves it alone");
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));

    ui.close_from_tab(&working);
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Close chat");
    ui.wait("it to close", |ui| ui.open_chats() == [first.clone()]);
    ui.wait("the other chat to show", |ui| ui.active_chat() == first);
}

#[gpui_kit::test]
fn a_tabs_close_button_shows_under_the_pointer_and_closes_only_that_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    // Working, so closing it asks first, which holds it open long enough to see which chat is showing.
    let working = ui.active_chat();
    ui.input("slow:200:still going");
    ui.press("enter");
    ui.wait("it to be working", |ui| ui.session_status() == Some(SessionStatus::Running));
    let showing = ui.new_chat();
    let close = |chat: &SessionId| SharedString::from(format!("close-chat-{chat}"));
    // The new tab came where the new chat button was, under the pointer.
    assert!(ui.visible(close(&showing)), "the pointer is over the new tab");
    ui.hover("send");
    assert!(!ui.visible(close(&working)) && !ui.visible(close(&showing)), "not even the chat showing has one");
    ui.hover(SharedString::from(format!("chat-{working}")));
    assert!(ui.visible(close(&working)), "the pointer is over its tab");
    assert!(!ui.visible(close(&showing)), "only the tab under the pointer has one");

    ui.click(close(&working));
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    assert_eq!(ui.active_chat(), showing, "pressing a tab's close button doesn't also show its chat");
    ui.cx.simulate_prompt_answer("Close chat");
    ui.wait("it to close", |ui| ui.open_chats() == [showing.clone()]);
    assert_eq!(ui.active_chat(), showing);
}
