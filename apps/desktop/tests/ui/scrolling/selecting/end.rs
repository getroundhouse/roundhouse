//! Selecting at the end of the chat: what the agent is still writing, and a prompt it hasn't read yet, which are
//! selected with the rest, and stay selected once they're recorded, or taken, in the rows they move to. Each moves as
//! the hub records what the agent does, which a fake hub's fixed chat can't, so this is on a real hub.

use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, TestAppContext};
use protocol::model::{SessionId, SessionStatus};
use transcript::Transcript;

use super::kinds::thought;
use super::{
    LAST, click, copy, everything, on_the_end_of_the_chat, on_the_reply_saying, open, settled, shift_click, tail,
};
use crate::support::{Gate, Stack, Ui, wait_until};

/// One chat, in two steps. Cmd-A while the agent writes a reply selects as far as it's got, and what's selected of
/// the reply stays selected once it's recorded, in its record's row. Then a prompt sent while the agent thinks waits
/// at the end of the chat, after the thought it's writing, until the agent takes it once the thought's done. It's
/// copied after the thought there, and from the row where it was taken once it has been, which is after the thought
/// too, where it's drawn, as a new selection of everything has it.
#[gpui_kit::test]
fn what_the_agent_is_writing_and_a_prompt_it_has_not_read_are_selected_with_the_rest_and_once_recorded(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("say:{LAST}"));
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);

    what_the_agent_is_writing(&mut ui, &stack, &session, window);
    a_prompt_the_agent_has_not_read(&mut ui, &stack, &session);
}

/// The first step: a reply that goes on until the test has selected some of it.
fn what_the_agent_is_writing(ui: &mut Ui<'_>, stack: &Stack, session: &SessionId, window: Bounds<Pixels>) {
    let transcript = ui.transcript().unwrap();
    let reply_done = Gate::new(stack, "reply-done");
    let writing_slowly = format!("slow-until:{reply_done}:word");
    stack.client().session_prompt(session, &writing_slowly, None).unwrap();
    ui.wait("writing: the start of the reply to be drawn", |ui| drawn(ui, &transcript, "word3"));

    let on_it = on_the_reply_saying(ui, window, LAST);
    let all = everything(ui, on_it);
    // From the reply before to a little way into this one.
    let (on_it, writing) = (on_the_reply_saying(ui, window, LAST), on_the_end_of_the_chat(ui, window));
    click(ui, on_it, 1);
    shift_click(ui, writing);
    let some = copy(ui);
    let unrecorded = still_writing(ui, &transcript);
    reply_done.open();
    wait_until("writing: the turn to end", || {
        stack.client().session_get(session).unwrap().status == SessionStatus::Idle
    });
    ui.wait_idle_with("writing: the recorded reply", |text| text.contains("word3"));
    settled(ui, stack, session);
    let recorded = copy(ui);

    let start = format!("say:{LAST}\n\n{LAST}\n\n{writing_slowly}\n\nword0 word1 word2 word3");
    assert!(unrecorded, "the reply was still being written, so that's what was selected");
    assert!(all.starts_with(&start), "what it's written so far, after the rest: {:?}", tail(&all));
    let into = some.rsplit("\n\n").next().unwrap();
    assert!(!into.is_empty() && "word0 word1".starts_with(into), "a little of it: {:?}", tail(&some));
    assert_eq!(recorded, some, "the same, once it's recorded");
}

/// The second step: a thought that goes on until the test has copied the prompt waiting after it.
fn a_prompt_the_agent_has_not_read(ui: &mut Ui<'_>, stack: &Stack, session: &SessionId) {
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    let window = settled(ui, stack, session);
    let thought_done = Gate::new(stack, "thought-done");
    let thinking = format!("think-until:{thought_done}:idea");
    // The step before left the focus in the chat.
    ui.click("composer");
    ui.paste_text(&thinking);
    ui.press("enter");
    ui.wait("waiting: the thought to be drawn as it's written", |ui| drawn(ui, &transcript, "idea3"));
    let steered = "say:Then the sidings.";
    ui.input(steered);
    ui.press("enter");
    ui.wait("waiting: the prompt to be drawn at the end", |ui| drawn(ui, &transcript, steered));

    let on_it = on_the_reply_saying(ui, window, LAST);
    let waiting = everything(ui, on_it);
    // The agent takes the prompt once it's done thinking.
    let untaken = still_writing(ui, &transcript);
    thought_done.open_once_the_agent_has_a_prompt();
    ui.wait_idle_with("waiting: the answer", |text| text.contains("Then the sidings."));
    let window = settled(ui, stack, session);
    let taken = copy(ui);
    let on_it = on_the_reply_saying(ui, window, "Then the sidings.");
    let all = everything(ui, on_it);

    assert!(untaken, "the agent was still thinking, so the prompt was waiting");
    let started = format!("\n\n{thinking}\n\nidea0 idea1 idea2 idea3");
    let Some(before) = waiting.find(&started) else { panic!("the thought: {:?}", tail(&waiting)) };
    assert!(waiting.ends_with(&format!("\n\n{steered}")), "then the prompt: {:?}", tail(&waiting));
    // Drawn, the thought doesn't end with the space it was recorded with.
    let thought = thought(ui);
    let thought = thought.trim_end();
    assert!(taken.contains(thought), "all of the thought, once it's done: {:?}", tail(&taken));
    assert!(taken.ends_with(&format!("\n\n{steered}")), "then the prompt, where it was taken: {:?}", tail(&taken));
    let answered = format!("{}\n\n{thinking}\n\n{thought}\n\n{steered}\n\nThen the sidings.", &waiting[..before]);
    assert_eq!(all, answered, "and a new selection of everything has it there, in the order they're drawn");
}

/// Whether the agent is still writing something the session's log hasn't recorded.
fn still_writing(ui: &Ui<'_>, transcript: &Entity<Transcript>) -> bool {
    ui.cx.read_entity(transcript, |t, _| !t.streaming_text().is_empty())
}

/// Whether the transcript has drawn a text that says `text` from the kit's blocks.
fn drawn(ui: &Ui<'_>, transcript: &Entity<Transcript>, text: &str) -> bool {
    ui.cx.read_entity(transcript, |t, _| t.kept_texts()).iter().any(|kept| kept.contains(text))
}
