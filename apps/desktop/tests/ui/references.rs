//! Issues and pull requests a reply links to, drawn as chips from what the hub found of each: asked for as they're
//! drawn, and again while they're in view, and not once they're out of it.

#![cfg(test)]

use client_core::fake_hub::events::{replied, taken, text, user_prompt};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{GithubRepositoryName, IssueNumber, IssueState, Reference, ReferenceKind, SessionStatus};
use transcript::REFERENCES_ASKED_AGAIN_AFTER;

use crate::support::{one_chat, open_on_the_chat};

/// An open issue, #1 of `example/signals`.
fn issue() -> Reference {
    Reference {
        repository: GithubRepositoryName::new("example", "signals").unwrap(),
        number: IssueNumber::try_from(1).unwrap(),
        kind: ReferenceKind::Issue { state: IssueState::Open },
        url: "https://github.com/example/signals/issues/1".to_string(),
        title: "Keep the signals' state on the hub".to_string(),
        author: Some("ada".to_string()),
        opened_at_ms: 0,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    }
}

/// The chat's first reply links to #1, and replies enough to be screens long come after it, so it's out of view at the
/// chat's end.
#[gpui_kit::test]
fn a_reference_is_asked_for_again_while_it_is_in_view_and_not_once_it_is_out_of_it(cx: &mut TestAppContext) {
    let mut events = vec![user_prompt("Where's the signal's state kept?"), taken(protocol::model::Seq::new(1))];
    events.push(text(
        "msg-1",
        "It's on the client for now, which [#1](https://github.com/example/signals/issues/1) is to change.",
    ));
    for i in 2..40 {
        events.push(text(&format!("msg-{i}"), &"The block ahead is clear, so the train goes on. ".repeat(8)));
    }
    events.push(replied("Done.", 9, 0.04));
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    world.references = vec![issue()];
    let asked = world.references_asked.clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let transcript = ui.transcript().unwrap();

    ui.cx.update_entity(&transcript, |t, cx| {
        t.list().go_to_row(0);
        cx.notify();
    });
    ui.wait("#1 to be asked for, in view", |_| asked.lock().len() == 1);
    ui.cx.executor().advance_clock(REFERENCES_ASKED_AGAIN_AFTER);
    ui.wait("#1 to be asked for again, still in view", |_| asked.lock().len() == 2);

    let last = ui.cx.read_entity(&transcript, |t, _| t.row_count() - 1);
    ui.cx.update_entity(&transcript, |t, cx| {
        t.list().go_to_row(last);
        cx.notify();
    });
    ui.frame();
    for _ in 0..3 {
        ui.cx.executor().advance_clock(REFERENCES_ASKED_AGAIN_AFTER);
        ui.frame();
    }
    assert_eq!(asked.lock().len(), 2, "out of view, it's asked for no more");
}

/// A chip is the words of the link it was written as, so a click on it opens that link, to where it goes, such as a
/// comment, as a click on any link does.
#[gpui_kit::test]
fn a_click_on_a_chip_opens_its_link(cx: &mut TestAppContext) {
    let link = "https://github.com/example/signals/issues/1#issuecomment-9";
    let reply = format!("It's on the client for now, as [the issue]({link}) says.");
    let events = vec![user_prompt("Where's the signal's state kept?"), text("msg-1", &reply)];
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    world.references = vec![issue()];
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let chip = "reference/example/signals#1";

    ui.wait("the chip", |ui| ui.has(chip));
    ui.click(chip);

    assert_eq!(ui.cx.opened_url().as_deref(), Some(link));
}
