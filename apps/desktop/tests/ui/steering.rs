//! Steering: Enter sends what's typed to an agent mid-turn, and the prompt waits until the agent takes it.

#![cfg(test)]

use client_core::session_log::PromptState;
use gpui_kit::{AppContext as _, Pixels, TestAppContext};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Place, Seq, SessionId, SessionStatus};
use protocol::prompt::Prompt;

use crate::queue::queued;
use crate::support::{Gate, Stack, Ui, top, wait_until};

/// A workspace whose agent is running a tool until the gate this answers is opened. A test that needs the turn to end
/// opens it or stops the turn, and otherwise the agent works until the stack goes.
pub(crate) fn working(ui: &mut Ui, stack: &Stack) -> Gate {
    let done = Gate::new(stack, "working-done");
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.paste_text(&format!("hang-until:{done}"));
    ui.press("enter");
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));
    done
}

#[gpui_kit::test]
fn enter_while_the_agent_works_steers_it_and_the_prompt_waits_until_its_taken(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    let first = working(&mut ui, &stack);

    // Its own tool, so the agent is still working once it has taken this.
    let second = Gate::new(&stack, "steered-done");
    ui.paste_text(&format!("hang-until:{second}; say:steered"));
    ui.press("enter");

    let transcript = ui.transcript().unwrap();
    let prompt = |ui: &Ui| {
        ui.events().into_iter().find(
            |e| matches!(&e.body, EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } if text.ends_with("steered")),
        )
    };
    ui.wait("the prompt to be recorded", |ui| prompt(ui).is_some());
    // Its row in the log, which isn't its place among the events drawn, since a row may hold no event.
    let at = ui.row_of(&prompt(&ui).unwrap());
    let state = |ui: &Ui| ui.cx.read_entity(&transcript, |t, _| t.prompt_state_at(at));
    assert_eq!(state(&ui), Some(PromptState::Waiting));
    assert!(queued(&ui).is_empty(), "steered, not queued");
    first.open_once_the_agent_has_a_prompt();
    ui.wait("the agent to take it", |ui| state(ui) == Some(PromptState::Taken));
    let ended = ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }));
    assert!(!ended, "taken because the agent said so, not because the turn is over");
    second.open();
    ui.wait_idle_with("the steered answer", |text| text.contains("steered"));
}

/// A frame is drawn from the log as the transcript last caught up with it, and the log can settle a prompt while the
/// frame is drawn, after a row has drawn it where it waited, its own or the end of the chat's. The row of the event
/// that settled it, drawn after that in the same frame, leaves it where it waited, rather than drawing it twice, and
/// the next frame draws it where it was taken.
#[gpui_kit::test]
fn a_prompt_taken_while_a_frame_is_drawn_is_drawn_once_in_that_frame(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    let first = working(&mut ui, &stack);
    ui.paste_text("say:steered");
    ui.press("enter");
    let transcript = ui.transcript().unwrap();
    let sent =
        |e: &&SessionEvent| matches!(&e.body, EventBody::UserPrompt { prompt, .. } if prompt.text == "say:steered");
    ui.wait("the prompt to be recorded", |ui| ui.events().iter().any(|e| sent(&e)));
    let sent = ui.events().iter().find(sent).cloned().unwrap();
    let (chat, own) = (ui.active_chat(), ui.row_of(&sent));
    let handle = ui.handle;

    ui.cx
        .update_window(handle, |_, window, cx| {
            transcript.update(cx, |t, cx| {
                t.start_a_frame();
                let end = t.list().row_count() - 1;
                for row in [own, end] {
                    t.draw_row_in_this_frame(row, window, cx);
                }
            });
        })
        .unwrap();
    first.open_once_the_agent_has_a_prompt();
    let taken = taking(&stack, &chat, sent.seq);
    let row = ui.row_of(&taken);
    wait_until("the app's log to have where the agent took it", || {
        ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some())
    });
    ui.cx
        .update_window(handle, |_, window, cx| transcript.update(cx, |t, cx| t.draw_row_in_this_frame(row, window, cx)))
        .unwrap();
    let copied = ui.cx.read_entity(&transcript, |t, _| t.all_as_copied());
    assert_eq!(copied.matches("say:steered").count(), 1, "drawn once in that frame: {copied:?}");

    ui.wait_idle_with("the steered answer", |text| text.contains("steered"));
    let copied = ui.cx.read_entity(&transcript, |t, _| t.all_as_copied());
    assert_eq!(copied.matches("say:steered").count(), 1, "and where it was taken after: {copied:?}");
}

/// The event that records the agent taking the prompt recorded at `prompt` in `chat`, once the hub has it.
fn taking(stack: &Stack, chat: &SessionId, prompt: Seq) -> SessionEvent {
    let took = |e: &SessionEvent| matches!(e.body, EventBody::PromptTaken { prompt: of } if of == prompt);
    let taken =
        || stack.client().session_read_places(chat, None, Place::new(1), 1000).unwrap().0.into_iter().find(took);
    wait_until("the agent to take the prompt", || taken().is_some());
    taken().unwrap()
}

/// How far down the window the lowest of `part`'s elements was drawn.
fn lowest(ui: &mut Ui, part: &str) -> Pixels {
    let drawn = ui.shown(part);
    drawn.into_iter().map(|id| top(ui, id)).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
}

/// Whether the prompt sent last is drawn under what the agent thought.
fn under_the_thought(ui: &mut Ui) -> bool {
    lowest(ui, "bubble/") > lowest(ui, "thinking/")
}

/// The thought began before the prompt came, so it's recorded after it, but the agent reads the prompt only after
/// the thought, and the prompt is drawn under it, at the end of the chat while it waits and where it was taken after.
/// It was drawn above the thought, where it was recorded, and a long thought hid it.
#[gpui_kit::test]
fn a_prompt_sent_while_the_agent_thinks_is_drawn_under_the_thought(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    let thought = Gate::new(&stack, "thought");
    ui.paste_text(&format!("think-until:{thought}:idea"));
    ui.press("enter");
    let thinking = |ui: &Ui, so_far: &str| ui.cx.read_entity(&transcript, |t, _| t.streaming_text().contains(so_far));
    ui.wait("the thought to stream", |ui| thinking(ui, "idea3"));

    ui.input("say:steered");
    ui.press("enter");

    assert!(under_the_thought(&mut ui), "as soon as it's sent, under the thought as it streams");
    let recorded = |ui: &Ui| {
        let events = ui.events();
        let prompt = events.iter().find(
            |e| matches!(&e.body, EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } if text == "say:steered"),
        );
        prompt.map(|event| {
            let row = ui.row_of(event);
            ui.cx.read_entity(&transcript, |t, _| t.prompt_state_at(row))
        })
    };
    ui.wait("the prompt to be recorded", |ui| recorded(ui).is_some());
    assert_eq!(recorded(&ui), Some(Some(PromptState::Waiting)));
    assert!(thinking(&ui, "idea"), "still thinking");
    assert!(under_the_thought(&mut ui), "recorded, and waiting under the thought as it streams");
    thought.open_once_the_agent_has_a_prompt();
    ui.wait("the agent to take it", |ui| recorded(ui) == Some(Some(PromptState::Taken)));
    assert!(under_the_thought(&mut ui), "taken, after the thought");
    ui.wait_idle_with("the steered answer", |text| text.contains("steered"));
    assert!(under_the_thought(&mut ui), "and once the turn is over");
}
