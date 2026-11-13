//! A request to use a tool, asked in the composer's place.

#![cfg(test)]

use gpui_kit::TestAppContext;
use protocol::event::{ApprovalOutcome, EventBody};
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui, id};

/// A removal the agent asks about even with full access, since what it removes is only known once it runs.
const REMOVAL: &str = r#"rm -rf "$(cat gone-path)""#;

/// What became of each request so far.
fn outcomes(ui: &Ui) -> Vec<ApprovalOutcome> {
    let outcome = |event: &protocol::event::SessionEvent| match &event.body {
        EventBody::ApprovalResolved { outcome, .. } => Some(*outcome),
        _ => None,
    };
    ui.events().iter().filter_map(outcome).collect()
}

/// A chat with full access, in a worktree with a directory `gone` for [`REMOVAL`] to remove. Making it isn't asked
/// about, which is what says the chat has full access.
fn with_something_to_remove<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, std::path::PathBuf) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let workspace = ui.new_workspace();
    let gone = ui.worktree(&workspace).join("gone");
    ui.input(r#"tool:mkdir gone && echo "$PWD/gone" > gone-path"#);
    ui.press("enter");
    ui.wait("the directory", |ui| ui.session_status() == Some(SessionStatus::Idle) && gone.exists());
    (ui, gone)
}

#[gpui_kit::test]
fn a_tools_use_is_asked_in_the_composers_place_and_denied_in_the_persons_words(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, gone) = with_something_to_remove(cx, &stack);

    // It's asked in the composer's place, with the command and why it's asked about, and what it's for apart from
    // them. Denying it is one of its answers, so there's nothing to skip it with.
    ui.ask(&format!("tool:{REMOVAL}"));
    assert!(!ui.has(id("add")), "the composer is put away");
    assert!(ui.shows_label("question-given", REMOVAL), "the command, whole");
    assert!(ui.has(id("question-reason")) && !ui.has(id("question-skip")));
    assert!(ui.shows_label("question-text", "Run a command the test asked for"), "what the agent said it's for");

    // Enter in the typing row sends only what's typed there: with Allow chosen and nothing typed, it sends nothing,
    // since the row says it denies.
    ui.click(id("question-answer-0"));
    ui.click(id("question-other"));
    ui.press("enter");
    ui.frame();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval), "nothing was sent");
    assert!(gone.exists() && outcomes(&ui).is_empty());

    // Nor does a sentence meant for the composer: a key that types something takes back the Allow its `1` chose, so
    // its Enter sends nothing.
    ui.let_what_is_asked_settle();
    for key in ["1", "x"] {
        ui.press(key);
    }
    ui.let_what_is_asked_settle();
    ui.press("enter");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval), "nothing was sent");
    assert!(gone.exists() && outcomes(&ui).is_empty());

    // What's typed there denies it in those words.
    ui.click(id("question-other"));
    ui.input("Remove it by its full path");
    ui.press("enter");
    ui.wait("the person's words", |ui| ui.told_to_agent().contains("Remove it by its full path"));
    ui.wait_for_the_turn();
    assert_eq!(outcomes(&ui), [ApprovalOutcome::Denied]);
    assert!(gone.exists(), "denied, so nothing was removed");
    assert!(ui.has(id("add")), "the composer is back");
}

#[gpui_kit::test]
fn subagents_that_ask_at_once_are_asked_in_turn_with_who_asks_and_how_many_wait(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, gone) = with_something_to_remove(cx, &stack);

    // Two subagents ask before either is answered. The first is asked, by what it's for, and the other waits.
    ui.ask(&format!("agents-ask:2:{REMOVAL}"));
    ui.wait("who asks", |ui| ui.shows_label("question-header", "Bash · Check 1"));
    ui.wait("the count of what waits", |ui| ui.shows_label("question-more-waiting", "1 more waiting"));

    // Denied, the next is asked, with nothing waiting behind it.
    ui.click(id("question-answer-1"));
    ui.click(id("question-send"));
    ui.wait("the second to be asked", |ui| ui.shows_label("question-header", "Bash · Check 2"));
    assert!(!ui.has(id("question-more-waiting")));
    assert!(gone.exists(), "the first was denied");

    // Allowed, its command runs, and the turn ends.
    ui.click(id("question-answer-0"));
    ui.click(id("question-send"));
    ui.wait("the removal", |ui| ui.session_status() == Some(SessionStatus::Idle) && !gone.exists());
    assert_eq!(outcomes(&ui), [ApprovalOutcome::Denied, ApprovalOutcome::Allowed]);
}
