//! Selecting at the end of the chat: what the agent is still writing, and a prompt it hasn't read yet, which are
//! selected with the rest, and stay selected once they're recorded, or taken, in the rows they move to.

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use protocol::model::SessionStatus;
use transcript::Transcript;

use super::{
    LAST, click, copy, everything, on_the_end_of_the_chat, on_the_reply_saying, open, settled, shift_click, tail,
};
use crate::support::{Stack, Ui, wait_until};

/// Cmd-A while the agent writes a reply selects as far as it's got. What's selected of the reply stays selected once
/// it's recorded, in its record's row.
#[gpui_kit::test]
fn what_the_agent_is_writing_is_selected_with_the_rest_and_stays_selected_once_it_s_recorded(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("say:{LAST}"));
    let mut ui = open(cx, &stack);
    let transcript = ui.transcript().unwrap();
    let window = settled(&mut ui, &stack, &session);
    stack.client().session_prompt(&session, "slow:30:word", None).unwrap();
    ui.wait("the start of the reply to be drawn", |ui| drawn(ui, &transcript, "word3"));

    let on_it = on_the_reply_saying(&ui, window, LAST);
    let all = everything(&mut ui, on_it);
    // From the reply before to a little way into this one.
    let (on_it, writing) = (on_the_reply_saying(&ui, window, LAST), on_the_end_of_the_chat(&ui, window));
    click(&mut ui, on_it, 1);
    shift_click(&mut ui, writing);
    let some = copy(&mut ui);
    let unrecorded = still_writing(&ui, &transcript);
    wait_until("the turn to end", || stack.client().session_get(&session).unwrap().status == SessionStatus::Idle);
    ui.wait_idle_with("the recorded reply", |text| text.contains("word29"));
    settled(&mut ui, &stack, &session);
    let recorded = copy(&mut ui);

    let start = format!("say:{LAST}\n\n{LAST}\n\nslow:30:word\n\nword0 word1 word2 word3");
    assert!(unrecorded, "the reply was still being written, so that's what was selected");
    assert!(all.starts_with(&start), "what it's written so far, after the rest: {:?}", tail(&all));
    let into = some.rsplit("\n\n").next().unwrap();
    assert!(!into.is_empty() && "word0 word1".starts_with(into), "a little of it: {:?}", tail(&some));
    assert_eq!(recorded, some, "the same, once it's recorded");
}

/// A prompt sent while the agent thinks waits at the end of the chat, after the thought it's writing, until the agent
/// takes it once the thought's done. It's copied after the thought there, and from the row where it was taken once
/// it has been.
#[gpui_kit::test]
fn a_prompt_the_agent_has_not_read_is_copied_after_what_it_s_writing_and_once_it_s_taken(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("say:{LAST}"));
    let mut ui = open(cx, &stack);
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    let window = settled(&mut ui, &stack, &session);
    ui.input("think:30:idea");
    ui.press("enter");
    ui.wait("the thought to be drawn as it's written", |ui| drawn(ui, &transcript, "idea3"));
    let steered = "say:Then the sidings.";
    ui.input(steered);
    ui.press("enter");
    ui.wait("the prompt to be drawn at the end", |ui| drawn(ui, &transcript, steered));

    let on_it = on_the_reply_saying(&ui, window, LAST);
    let waiting = everything(&mut ui, on_it);
    // The agent takes the prompt once it's done thinking.
    let untaken = still_writing(&ui, &transcript);
    ui.wait_idle_with("the answer", |text| text.contains("Then the sidings."));
    settled(&mut ui, &stack, &session);
    let taken = copy(&mut ui);

    assert!(untaken, "the agent was still thinking, so the prompt was waiting");
    assert!(waiting.contains("\n\nthink:30:idea\n\nidea0 idea1 idea2 idea3"), "the thought: {:?}", tail(&waiting));
    assert!(waiting.ends_with(&format!("\n\n{steered}")), "then the prompt: {:?}", tail(&waiting));
    assert!(taken.contains("idea29"), "all of the thought, once it's done: {:?}", tail(&taken));
    assert!(taken.ends_with(&format!("\n\n{steered}")), "then the prompt, where it was taken: {:?}", tail(&taken));
}

/// Whether the agent is still writing something the session's log hasn't recorded.
fn still_writing(ui: &Ui<'_>, transcript: &Entity<Transcript>) -> bool {
    ui.cx.read_entity(transcript, |t, _| !t.streaming_text().is_empty())
}

/// Whether the transcript has drawn a text that says `text` from the kit's blocks.
fn drawn(ui: &Ui<'_>, transcript: &Entity<Transcript>, text: &str) -> bool {
    ui.cx.read_entity(transcript, Transcript::kept_texts).iter().any(|kept| kept.contains(text))
}
