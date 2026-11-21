//! A chat: talking in it, what the agent asks, stopping it, what opens over it, the workspace's other chats, what's
//! typed and not sent, reading, plan mode, and copying from it by touch. A chat is known by its `composer`, and a
//! proposal over it by `send-proposal-answer`.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::TestAppContext;
use protocol::capability::settings::Settings;
use protocol::model::AgentKind;

use crate::support::{Phone, Stack, id};

/// A workspace opens on the chat it has, which another client made, rather than on one of the phone's own.
#[gpui_kit::test]
fn opens_a_listed_workspace_on_the_chat_it_has_and_talks_in_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let chat = stack.client().session_create(&workspace.id, AgentKind::Claude, Settings::default(), None).unwrap();
    let mut phone = Phone::paired(cx, &stack);

    phone.open_chat(&workspace);
    phone.send("say:hello from the phone");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("hello from the phone"));
    let sessions = stack.client().session_list(&workspace.id).unwrap();
    assert_eq!(sessions.iter().map(|s| &s.id).collect::<Vec<_>>(), [&chat.id], "the phone made no chat of its own");
    phone.tap("back");
    assert!(phone.has("hub"));
}

/// The chat's header names the workspace as its record does, which another client can change while it's open.
#[gpui_kit::test]
fn the_chats_header_names_the_workspace_as_its_record_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    assert_eq!(phone.chat_title().as_deref(), Some(workspace.label()));

    stack.client().workspace_rename(&workspace.id, "Renamed on the desktop").unwrap();

    phone.wait("the new title", |phone| phone.chat_title().as_deref() == Some("Renamed on the desktop"));
}

/// Whether the agent is working is the session's record's, which the chat takes as it changes, and the working line
/// it ends with says so.
#[gpui_kit::test]
fn the_chat_says_the_agent_is_working_until_its_turn_ends(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    assert!(!phone.has("working-line"));

    let done = stack.gate("done");
    phone.send(&format!("hang-until:{}; say:done", done.display()));
    phone.wait("the working line", |phone| phone.has("working-line"));

    std::fs::write(&done, "").unwrap();
    phone.wait("the turn to end", |phone| phone.assistant_text().contains("done") && !phone.has("working-line"));
}

/// The phone can't put a chat in plan mode, which asks before a call that writes, so the desktop did.
#[gpui_kit::test]
fn allows_a_tool_use_in_the_composers_place(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let planning = Settings { mode: Some(protocol::fixtures::plan_mode()), ..Settings::default() };
    stack.client().session_create(&workspace.id, AgentKind::Claude, planning, None).unwrap();
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);

    phone.send("say:starting; tool:echo changed >> a.txt; say:finished");
    phone.next_open_approval();
    phone.wait("the request to be asked", |phone| phone.has(id("question-answer-0")));
    assert!(!phone.has("composer"), "asked in the composer's place");
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's allowed");
    phone.tap(id("question-answer-0"));
    phone.tap(id("question-send"));

    phone.wait("the turn to finish", |phone| phone.assistant_text().contains("finished"));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");
    assert!(phone.has("composer"), "the composer is back");
}

#[gpui_kit::test]
fn answers_a_question_by_typing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send("ask:Which day?=Monday/Tuesday");
    phone.next_open_approval();
    phone.wait("the question", |phone| phone.has(id("question-other")));

    phone.tap(id("question-other"));
    phone.type_text("Friday, if that works");
    phone.tap(id("question-send"));

    phone.wait("the typed answer", |phone| phone.told_to_agent().contains(r#""Which day?"=Friday, if that works"#));
}

/// The list's connection, and a chat open on it, both carry on over the new one.
#[gpui_kit::test]
fn a_chat_carries_on_after_the_hub_restarts(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);

    stack.stop_hub();
    phone.wait("the connection to go", |phone| !phone.is_connected());
    stack.start_hub();
    phone.wait("the connection to come back", |phone| phone.is_connected());
    phone.send("say:after the restart");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("after the restart"));
    assert!(phone.has("composer"), "still in the chat");
}

/// While the agent works, Stop takes Send's place until something's typed, when Send comes back to steer with it, and
/// Stop ends the turn.
#[gpui_kit::test]
fn stops_the_agent_and_steers_once_something_is_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    let never = stack.gate("never");
    phone.send(&format!("hang-until:{}; say:never", never.display()));
    phone.wait("Stop", |phone| phone.has("stop") && !phone.has("send"));

    phone.type_into("composer", "say:steered");
    assert!(phone.has("send") && !phone.has("stop"), "something typed can be sent");
    phone.tap("send");
    phone.wait("the steer, in the chat while the agent works", |phone| {
        phone.prompts().iter().any(|prompt| prompt.ends_with("say:steered")) && phone.has("stop")
    });

    phone.tap("stop");
    phone.wait("the turn to end", |phone| !phone.has("working-line") && phone.has("send"));
}

/// What the fake agent proposes: one question with two options, the first recommended.
const PROPOSE: &str = r#"mcp:propose {"title":"Holding a train","context":"Trains wait a minute.","questions":[{"header":"Holding","question":"Where does the wait live?","options":[{"label":"The signal","description":"It counts down."},{"label":"The train"}],"recommended":"A","why":"It survives a restart."}]}"#;

#[gpui_kit::test]
fn opens_a_subagents_work_over_the_chat_and_goes_back_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send("agent-calls:1:Look around; say:done");

    // Its Open is there once the subagent's record is, which comes on the feed.
    phone.tap_starting("open-subagent/");
    phone.wait("the subagent's work, with nothing to type into", |phone| {
        phone.subagent_shown() && !phone.has("composer")
    });
    phone.tap("back");
    phone.wait("the chat again", |phone| !phone.subagent_shown() && phone.has("composer"));
}

#[gpui_kit::test]
fn answers_a_proposal_over_the_chat_keeping_what_was_chosen_while_back_in_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send(&format!("{PROPOSE}; say:done"));
    phone.tap_starting("open-proposal/");
    phone.wait("the proposal", |phone| phone.has("send-proposal-answer") && !phone.has("composer"));
    phone.tap("choose-0-1");

    phone.tap("back");
    phone.wait("the chat again", |phone| phone.has("composer"));
    phone.tap_starting("open-proposal/");
    phone.wait("the proposal again", |phone| phone.has("send-proposal-answer"));
    // Send does nothing until something's chosen, so the answer going is what was chosen being kept.
    phone.tap("send-proposal-answer");
    phone.wait("the answer, after which only what was chosen says so", |phone| !phone.has("choose-0-0"));
}

#[gpui_kit::test]
fn starts_another_chat_from_the_title_and_goes_back_to_the_first_with_what_was_typed_in_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    let first = phone.chat_shown().unwrap();
    phone.type_into("composer", "half a thought");

    phone.tap("chats");
    phone.choose("New chat");
    phone.wait("the new chat, with nothing typed in it", |phone| {
        phone.chat_shown().is_some_and(|shown| shown != first) && phone.typed().is_empty()
    });

    // Both are untitled, and the menu lists the open chats oldest first, as the desktop's tabs are.
    phone.tap("chats");
    phone.choose("Untitled");
    phone.wait("the first chat, with what was typed in it", |phone| {
        phone.chat_shown().as_ref() == Some(&first) && phone.typed() == "half a thought"
    });
}

#[gpui_kit::test]
fn a_draft_typed_in_another_client_shows_in_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    let chat = phone.chat_shown().unwrap();

    let elsewhere = client_core::Drafts::start(&stack.client());
    let draft = protocol::model::DraftToKeep { text: "typed on the desktop".into(), files: Vec::new() };
    elsewhere.set(client_core::DraftChange::Composer(chat, draft));

    phone.wait("the other client's draft", |phone| phone.typed() == "typed on the desktop");
}

#[gpui_kit::test]
fn a_reply_is_read_while_its_chat_shows_and_not_while_another_app_is_in_front(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    let chat = phone.chat_shown().unwrap();
    let client = stack.client();
    let at_the_hub = || client.session_get(&chat).unwrap();

    phone.send("say:first");
    phone.wait("the first reply, read as it comes", |_| {
        let chat = at_the_hub();
        chat.has_reply() && !chat.unread()
    });
    let first = at_the_hub().reply_seq;

    let later = stack.gate("later");
    phone.send(&format!("hang-until:{}; say:second", later.display()));
    phone.send_to_back();
    std::fs::write(&later, "").unwrap();
    // The phone decides whether it's read as its own record of the chat takes the reply, so a mark it shouldn't send
    // goes then, and reaches the hub within the bound.
    phone.wait("the second reply, on the phone", |phone| {
        phone.shown_record().is_some_and(|chat| chat.reply_seq > first)
    });
    waiting::thread::for_what_mustnt_happen(std::time::Duration::from_millis(300));
    assert!(at_the_hub().unread(), "a reply that came while another app was in front isn't read");

    phone.bring_to_front();
    phone.wait("the second reply, read once the app's in front", |_| !at_the_hub().unread());
}

#[gpui_kit::test]
fn turns_plan_mode_on_and_off_from_the_plus_menu(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    let chat = phone.chat_shown().unwrap();
    let client = stack.client();
    let planning = || client.session_get(&chat).unwrap().mode.plan;

    phone.tap("composer-menu");
    phone.choose("Plan mode");
    // On the phone too, since its menu offers what its record of the chat says, which can come after the hub has it.
    phone.wait("plan mode, as the hub has it", |phone| planning() && phone.has("planning"));

    phone.tap("composer-menu");
    phone.choose("Plan mode");
    phone.wait("plan mode off again", |phone| !planning() && !phone.has("planning"));
}

/// A long press on a prompt, a call, a code block or an open command opens a menu with Copy first, which copies what was
/// typed, what the call acted on, the code, or the command, and leaves the call as it was.
#[gpui_kit::test]
fn a_long_press_copies_a_prompt_what_a_call_acted_on_or_code(cx: &mut TestAppContext) {
    // The fence is after `say:`, so only the reply, which the fence starts, has a code block, left open to its end.
    const SENT: &str = "edit:1; say:```sh\necho hi";
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send(SENT);
    phone.wait("the turn to finish", |phone| phone.assistant_text().contains("echo hi"));

    let bubble = phone.found_starting("bubble/").expect("the prompt");
    phone.press_long(bubble);
    phone.choose("Copy");
    assert_eq!(phone.copied().as_deref(), Some(SENT), "what was typed");

    assert_eq!(phone.expanded("Edit"), Some(false));
    let call = phone.found_starting("call/").expect("the edit's line");
    phone.press_long(call);
    phone.choose("Copy");
    let copied = phone.copied().unwrap_or_default();
    assert!(copied.ends_with("notes.txt"), "the file it edited, not {copied:?}");
    assert_eq!(phone.expanded("Edit"), Some(false), "a long press doesn't open the call");

    let code = phone.found_numbered("codeblock").expect("the code block");
    phone.press_long(code);
    phone.choose("Copy");
    assert_eq!(phone.copied().as_deref(), Some("echo hi"), "its code");

    phone.send("tool:echo ran; say:ran it");
    phone.wait("the command to run", |phone| phone.assistant_text().contains("ran it"));
    phone.tap_labelled("Run");
    phone.wait("the command's call to open", |phone| phone.expanded("Run") == Some(true));
    let command = phone.found_starting("command/").expect("the command, open");
    phone.press_long(command);
    phone.choose("Copy");
    assert_eq!(phone.copied().as_deref(), Some("echo ran"), "the command");
}
