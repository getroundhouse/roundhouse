//! Notifications and their sound, which tell of a chat that comes to need the person, unless it's the one being read.

#![cfg(test)]

use gpui_kit::{SharedString, SystemNotification, SystemNotificationResponse, TestAppContext};
use protocol::model::{SessionId, WorkspaceId};

use crate::settings::open_settings;
use crate::support::{Stack, Ui};

/// The notifications shown so far whose title is this workspace's name.
fn told_of(ui: &mut Ui<'_>, workspace: &WorkspaceId) -> Vec<SystemNotification> {
    let name = ui.view().records().workspace(workspace).unwrap().label().to_string();
    ui.cx.shown_system_notifications().into_iter().filter(|note| note.title == name).collect()
}

fn sounds_played(ui: &mut Ui<'_>) -> Vec<String> {
    ui.cx.read(desktop::sounds_played)
}

/// A window in front, with a repository and a workspace in it, then another showing in its place. Returns the first
/// workspace and its chat, which isn't showing.
fn open_with_one_elsewhere<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, WorkspaceId, SessionId) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.bring_to_front();
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let elsewhere = ui.new_workspace();
    let chat = ui.active_chat();
    ui.new_workspace();
    (ui, elsewhere, chat)
}

#[gpui_kit::test]
fn a_reply_elsewhere_is_told_of_with_a_sound_and_one_in_the_chat_being_read_isnt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_one_elsewhere(cx, &stack);
    let showing = ui.view().selected.clone().unwrap();

    stack.client().session_prompt(&chat, "say:while you were away", None).unwrap();
    ui.wait("a notification", |ui| !told_of(ui, &elsewhere).is_empty());
    ui.input("say:right here");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("right here"));

    let notes = told_of(&mut ui, &elsewhere);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].body.starts_with("Finished"), "says why: {}", notes[0].body);
    assert!(told_of(&mut ui, &showing).is_empty(), "the chat being read isn't told of");
    assert_eq!(sounds_played(&mut ui), ["Chime"]);
}

#[gpui_kit::test]
fn with_another_app_in_front_a_reply_in_the_chat_showing_is_told_of(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _, _) = open_with_one_elsewhere(cx, &stack);
    let showing = ui.view().selected.clone().unwrap();
    let chat = ui.active_chat();

    ui.send_to_back();
    stack.client().session_prompt(&chat, "say:while you were in another app", None).unwrap();

    ui.wait("a notification", |ui| !told_of(ui, &showing).is_empty());
}

/// The notification of this workspace's whose body has `said` in it, once there is one.
fn told_of_saying(ui: &mut Ui<'_>, workspace: &WorkspaceId, said: &str) -> SystemNotification {
    ui.wait("the notification", |ui| told_of(ui, workspace).iter().any(|note| note.body.contains(said)));
    told_of(ui, workspace).into_iter().find(|note| note.body.contains(said)).unwrap()
}

fn taken_back(ui: &mut Ui<'_>, note: &SystemNotification) -> bool {
    ui.cx.dismissed_system_notifications().contains(&note.tag)
}

#[gpui_kit::test]
fn clicking_a_notification_shows_its_chat_and_reads_no_other_on_the_way(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.bring_to_front();
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let elsewhere = ui.new_workspace();
    let showing_there = ui.active_chat();
    let clicked = ui.new_chat();
    ui.show_chat(&showing_there);
    ui.new_workspace();
    stack.client().session_prompt(&showing_there, "say:for the chat showing there", None).unwrap();
    stack.client().session_prompt(&clicked, "say:for the chat clicked", None).unwrap();
    told_of_saying(&mut ui, &elsewhere, "showing there");
    let note = told_of_saying(&mut ui, &elsewhere, "clicked");

    ui.cx.simulate_system_notification_response(SystemNotificationResponse { tag: note.tag, action_id: None });

    ui.wait("its workspace to show", |ui| ui.view().selected.as_ref() == Some(&elsewhere));
    ui.wait("its chat to show", |ui| ui.active_chat() == clicked);
    ui.wait("the hub to have it read", |_| !stack.client().session_get(&clicked).unwrap().unread());
    assert!(stack.client().session_get(&showing_there).unwrap().unread(), "the chat showing there wasn't read");
}

#[gpui_kit::test]
fn reading_a_chat_takes_its_notification_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_one_elsewhere(cx, &stack);
    stack.client().session_prompt(&chat, "say:read me", None).unwrap();
    let note = told_of_saying(&mut ui, &elsewhere, "Finished");

    ui.click(SharedString::from(format!("workspace-{elsewhere}")));

    ui.wait("the notification to go", |ui| taken_back(ui, &note));
}

#[gpui_kit::test]
fn showing_a_chat_that_waits_on_you_takes_its_notification_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, showing) = open_with_one_elsewhere(cx, &stack);
    ui.click(SharedString::from(format!("workspace-{workspace}")));
    ui.wait("it to show", |ui| ui.view().selected.as_ref() == Some(&workspace));
    let asking = ui.new_chat();
    ui.show_chat(&showing);
    stack.client().session_prompt(&asking, "ask:Red or blue?=Red/Blue", None).unwrap();
    let note = told_of_saying(&mut ui, &workspace, "Waiting on you");

    ui.show_chat(&asking);

    ui.wait("the notification to go", |ui| taken_back(ui, &note));
}

/// Two chats coming to need you one after another, each told of on its own, sound once while the app's clock stands
/// still, which it does in a test until it's moved.
#[gpui_kit::test]
fn chats_that_need_you_within_two_seconds_sound_once_and_later_sound_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, first) = open_with_one_elsewhere(cx, &stack);
    let second = ui.new_workspace();
    let second_chat = ui.active_chat();
    ui.new_workspace();
    stack.client().session_prompt(&first, "say:one", None).unwrap();
    told_of_saying(&mut ui, &elsewhere, "one");
    stack.client().session_prompt(&second_chat, "say:two", None).unwrap();
    told_of_saying(&mut ui, &second, "two");
    assert_eq!(sounds_played(&mut ui), ["Chime"]);

    ui.cx.executor().advance_clock(desktop::app::SOUNDS_APART);
    stack.client().session_prompt(&first, "say:again", None).unwrap();

    ui.wait("it to sound again", |ui| sounds_played(ui) == ["Chime", "Chime"]);
}

#[gpui_kit::test]
fn turned_off_nothing_is_told_of_and_the_sound_still_plays(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_one_elsewhere(cx, &stack);
    open_settings(&mut ui);
    ui.click("settings-page-notifications");
    ui.click_label("Notify when an agent needs you");

    stack.client().session_prompt(&chat, "say:quietly", None).unwrap();

    ui.wait("the sound", |ui| !sounds_played(ui).is_empty());
    assert!(told_of(&mut ui, &elsewhere).is_empty());
}

#[gpui_kit::test]
fn choosing_a_sound_plays_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    open_settings(&mut ui);
    ui.click("settings-page-notifications");

    ui.click("settings-sound");
    ui.click_menu_item("Chime");

    ui.wait("it to play", |ui| sounds_played(ui) == ["Chime"]);
}
