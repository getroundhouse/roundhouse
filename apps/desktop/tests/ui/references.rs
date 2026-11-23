//! Issues and pull requests a reply links to, drawn as chips from what the hub found of each: shown, as the app tells
//! the hub, while they're in view, and not once they're out of it.

#![cfg(test)]

use client_core::fake_hub::events::{first_turn, replied, text, user_prompt};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{Level, Reference, SessionStatus};

use crate::support::{one_chat, open_on_the_chat};

/// An open issue, #1 of `example/signals`.
fn issue() -> Reference {
    protocol::fixtures::issue("example/signals", 1, "Keep the signals' state on the hub")
}

/// The chat's first reply links to #1, and replies enough to be screens long come after it, so it's out of view at the
/// chat's end.
#[gpui_kit::test]
fn a_reference_is_shown_while_it_is_in_view_and_not_once_it_is_out_of_it(cx: &mut TestAppContext) {
    let mut events = first_turn(user_prompt("Where's the signal's state kept?"), []);
    events.push(text(
        "msg-1",
        "It's on the client for now, which [#1](https://github.com/example/signals/issues/1) is to change.",
    ));
    for i in 2..40 {
        events.push(text(&format!("msg-{i}"), &"The block ahead is clear, so the train goes on. ".repeat(8)));
    }
    events.push(replied("Done.", 9, 0.04));
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    world.github_says.references = vec![issue()];
    let following = world.following.clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let transcript = ui.transcript().unwrap();
    let shows_it =
        move || following.any(|level| matches!(level, Level::Reference { reference } if reference.number.get() == 1));

    ui.cx.update_entity(&transcript, |t, cx| {
        t.list().go_to_row(0);
        cx.notify();
    });
    ui.wait("#1 to be shown, in view", |_| shows_it());
    ui.wait("its chip", |ui| ui.has("reference/example/signals#1"));

    let last = ui.cx.read_entity(&transcript, |t, _| t.row_count() - 1);
    ui.cx.update_entity(&transcript, |t, cx| {
        t.list().go_to_row(last);
        cx.notify();
    });
    ui.wait("#1 to be shown no more, out of view", |_| !shows_it());
}

/// A chat behind another's tab shows none of its chips, so what the hub reads for the app is what's on screen, and its
/// chips are shown again once its tab is in front again.
#[gpui_kit::test]
fn a_chat_behind_another_shows_none_of_its_chips(cx: &mut TestAppContext) {
    let reply = "It's on the client for now, which [#1](https://github.com/example/signals/issues/1) is to change.";
    let (mut world, chat) = one_chat(SessionStatus::Idle, vec![user_prompt("Where's it kept?"), text("msg-1", reply)]);
    let workspace = world.sessions[0].workspace_id.clone();
    let other = world.chat(&workspace, SessionStatus::Idle, vec![user_prompt("Anything else?")]);
    world.github_says.references = vec![issue()];
    let following = world.following.clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let shows_it =
        move || following.any(|level| matches!(level, Level::Reference { reference } if reference.number.get() == 1));
    ui.wait("#1 to be shown", |_| shows_it());

    ui.activate_chat(&other);
    ui.wait("#1 to be shown no more, behind the other chat", |_| !shows_it());

    ui.activate_chat(&chat);
    ui.wait("#1 to be shown again, in front again", |_| shows_it());
}

/// A chip is the words of the link it was written as, so a click on it opens that link, to where it goes, such as a
/// comment, as a click on any link does.
#[gpui_kit::test]
fn a_click_on_a_chip_opens_its_link(cx: &mut TestAppContext) {
    let link = "https://github.com/example/signals/issues/1#issuecomment-9";
    let reply = format!("It's on the client for now, as [the issue]({link}) says.");
    let events = vec![user_prompt("Where's the signal's state kept?"), text("msg-1", &reply)];
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    world.github_says.references = vec![issue()];
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    let chip = "reference/example/signals#1";

    ui.wait("the chip", |ui| ui.has(chip));
    ui.click(chip);

    assert_eq!(ui.cx.opened_url().as_deref(), Some(link));
}
