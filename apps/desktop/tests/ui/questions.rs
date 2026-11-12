//! Questions from the agent, asked in the composer's place.

#![cfg(test)]

use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

/// The line a question leaves in the transcript once it's no longer waiting, for one question and for two.
const ASKED: &str = "Asked";
const ASKED_TWO: &str = "Asked 2 questions";

/// The result of the agent's question, from what it was told.
fn told_to_agent(ui: &Ui) -> String {
    let mut told = String::new();
    for event in ui.events() {
        if let EventBody::AgentUser { blocks, .. } = &event.body {
            for block in blocks {
                if let protocol::event::ContentBlock::ToolResult { text, .. } = block {
                    told.push_str(text);
                }
            }
        }
    }
    told
}

fn open_question(ui: &mut Ui, prompt: &str) {
    ui.input(prompt);
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval();
    ui.wait("the question to show", |ui| ui.has(id("question")));
}

fn id(name: &str) -> SharedString {
    SharedString::from(name.to_string())
}

/// Lets the time pass after which a new question takes keys.
fn let_it_settle(ui: &mut Ui) {
    ui.cx.executor().advance_clock(transcript::QUESTION_SETTLES);
    ui.frame();
}

fn wait_for_the_turn(ui: &mut Ui) {
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
}

#[gpui_kit::test]
fn a_question_is_answered_by_choosing_or_typing_or_skipped(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();

    // It takes the composer's place, and a choice waits for the arrow, so it can be changed first.
    open_question(&mut ui, "ask:Red or blue?=Red/Blue");
    assert!(!ui.has(id("add")), "the composer is put away");
    ui.click(id("question-answer-0"));
    ui.click(id("question-answer-1"));
    ui.click(id("question-send"));
    ui.wait("the answer", |ui| told_to_agent(ui).contains(r#""Red or blue?"=Blue"#));
    wait_for_the_turn(&mut ui);
    assert!(ui.has(id("add")), "the composer is back");

    // Typed in the row under the options, it's the answer, and Enter sends it with nothing left to answer. Focus goes
    // back to the composer, and the answer isn't sent as a prompt too.
    open_question(&mut ui, "ask:Which day?=Monday/Tuesday");
    ui.click(id("question-other"));
    ui.input("Friday, if that works");
    ui.press("enter");
    ui.wait("the typed answer", |ui| told_to_agent(ui).contains(r#""Which day?"=Friday, if that works"#));
    wait_for_the_turn(&mut ui);
    ui.input("say:back");
    assert_eq!(ui.composer_text(), "say:back");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 2, "the answer wasn't sent as a prompt too");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("back"));

    // It takes focus from the composer, and keys that come straight away are taken as typing meant for the composer.
    // Once it settles, a number chooses and Enter sends.
    open_question(&mut ui, "ask:Tea or coffee?=Tea/Coffee");
    ui.press("1");
    ui.press("enter");
    let_it_settle(&mut ui);
    ui.press("2");
    ui.press("enter");
    ui.wait("the answer by keys", |ui| told_to_agent(ui).contains(r#""Tea or coffee?"=Coffee"#));
    assert!(!told_to_agent(&ui).contains("=Tea"), "keys before it settled chose nothing");
    wait_for_the_turn(&mut ui);

    // Skipping lets the agent know, and once it's over, typing is a new message again.
    open_question(&mut ui, "ask:Now or later?=Now/Later");
    ui.click(id("question-skip"));
    ui.wait("the skip", |ui| told_to_agent(ui).contains("chose not to answer"));
    wait_for_the_turn(&mut ui);
    ui.input("say:a new message");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("a new message"));
}

#[gpui_kit::test]
fn an_answered_question_stays_as_a_line_that_opens_to_what_was_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();

    // While it waits, it's asked in the composer's place, and the transcript leaves it out.
    open_question(&mut ui, "ask:Red or blue?=Red/Blue|Why?=Because");
    assert!(ui.labelled(ASKED_TWO).is_empty(), "not in the transcript while it waits");

    // Once it's answered, here only the first of the two, it's a line, and so is one that was skipped. The turn's end
    // can reach the app before the record that closes the question does, so each waits for its line, which comes
    // once the question has gone from the composer's place.
    ui.click(id("question-answer-1"));
    ui.click(id("question-send"));
    ui.wait("the answered question's line", |ui| ui.labelled(ASKED_TWO).len() == 1);
    wait_for_the_turn(&mut ui);
    open_question(&mut ui, "ask:Now or later?=Now/Later");
    ui.click(id("question-skip"));
    ui.wait("the skipped question's line", |ui| ui.labelled(ASKED).len() == 1);
    wait_for_the_turn(&mut ui);
    assert_eq!(ui.labelled(ASKED_TWO).len(), 1, "the first is still a line of its own");
    assert!(ui.shown("asked/").is_empty(), "each starts closed");

    // Opened, each says what was answered to each question, or that nobody answered it.
    ui.click_labelled(ASKED_TWO, 0);
    assert_eq!(ui.shown("asked/").len(), 1);
    assert_eq!(ui.shown("answer-0/").len(), 1, "what was answered to the first");
    assert_eq!(ui.shown("not-answered-1/").len(), 1, "and that nobody answered the second");
    ui.click_labelled(ASKED, 0);
    assert_eq!(ui.shown("not-answered-0/").len(), 1, "the one that was skipped");
}

#[gpui_kit::test]
fn several_questions_are_asked_one_at_a_time(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    open_question(&mut ui, "ask:*Which colors?=Red/Green/Blue|How many?=One/Two|Why?=Because");
    // Only the first question has a third option.
    let showing_the_first = |ui: &mut Ui| ui.has(id("question-answer-2"));

    // Nothing's chosen yet, so the arrow sends nothing, which the answers at the end show.
    ui.click(id("question-send"));

    // Several can be chosen for the first, and since more could follow, it stays.
    ui.click(id("question-answer-0"));
    ui.click(id("question-answer-2"));
    assert!(showing_the_first(&mut ui));

    // Enter goes on to the next without an answer. That one takes one answer, so choosing it goes on again.
    ui.press("enter");
    assert!(!showing_the_first(&mut ui));
    ui.click(id("question-answer-1"));
    // The third has only one option.
    assert!(!ui.has(id("question-answer-1")), "went on to the third");
    ui.click(id("question-back"));
    assert!(ui.has(id("question-answer-1")) && !showing_the_first(&mut ui), "back to the second");
    ui.click(id("question-dot-0"));
    assert!(showing_the_first(&mut ui));

    // The arrow sends what's answered, and leaves out the third, which isn't.
    ui.click(id("question-send"));
    ui.wait("the answers", |ui| told_to_agent(ui).contains(r#""How many?"=Two"#));
    let told = told_to_agent(&ui);
    assert!(told.contains(r#""Which colors?"=Red, Blue"#), "{told}");
    assert!(!told.contains("Why?"), "{told}");
}

#[gpui_kit::test]
fn the_keys_move_between_questions_and_into_and_out_of_typing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    open_question(&mut ui, "ask:*Which colors?=Red/Green/Blue|How many?=One/Two");
    let_it_settle(&mut ui);
    let showing_the_first = |ui: &mut Ui| ui.has(id("question-answer-2"));

    // The arrows move between questions.
    ui.press("right");
    assert!(!showing_the_first(&mut ui));
    ui.press("left");
    assert!(showing_the_first(&mut ui));

    // 0 goes to the typing row, and its checkbox takes back what's typed there.
    ui.press("0");
    ui.input("Teal");
    ui.click(id("question-other-check"));

    // Escape comes back out, so a number chooses again rather than being typed.
    ui.press("escape");
    ui.press("1");
    ui.press("right");
    ui.press("2");
    ui.press("enter");
    ui.wait("the answers", |ui| told_to_agent(ui).contains(r#""How many?"=Two"#));
    let told = told_to_agent(&ui);
    assert!(told.contains(r#""Which colors?"=Red"#) && !told.contains("Teal"), "{told}");
}

#[gpui_kit::test]
fn closing_settings_gives_focus_to_a_question_that_came_meanwhile(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.wait("the chat", |ui| ui.pane().is_some_and(|pane| ui.cx.read_entity(&pane, |p, _| p.active.is_some())));
    let pane = ui.pane().unwrap();
    let chat = ui.cx.read_entity(&pane, |p, _| p.active.clone()).unwrap();

    // Settings keeps the composer's focus to give back, and the question comes while it shows.
    ui.press("cmd-,");
    stack.client().send_prompt_carrying(&chat, "ask:Now or later?=Now/Later", None, &[]).unwrap();
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval();
    ui.press("escape");
    ui.wait("the question to show", |ui| ui.has(id("question")));

    let_it_settle(&mut ui);
    ui.press("2");
    ui.press("enter");
    ui.wait("the answer by keys", |ui| told_to_agent(ui).contains(r#""Now or later?"=Later"#));
}
