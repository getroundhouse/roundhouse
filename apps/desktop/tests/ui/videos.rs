//! Videos a reply points to, which play where they are. The worker keeps a reply's videos, and how the window plays
//! them is the app's, so they're played on a fake hub holding made-up replies, with no worker to wait for. Each frame
//! is decoded on the player's own thread, by the system's clock, while the window takes each as it's due by the test's,
//! which the test moves.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, taken, text_pointing, user_prompt};
use gpui_kit::TestAppContext;
use protocol::attachment::{Attachment, MediaType};
use protocol::event::LinkedImage;
use protocol::model::{AttachmentId, Seq, SessionStatus};
use video::fixtures::{TWO_SECONDS, UNKNOWN_LENGTH};

use crate::support::{FakeHub, Ui};

/// How far the test's clock moves for each frame it draws while a video plays, a little more than a frame's worth of
/// the fixture's ten a second.
const STEP: Duration = Duration::from_millis(120);

/// The window on a fake hub holding a chat whose one reply points to a two-second WebM, the copy the hub kept. The hub
/// goes with the test, so it's returned to be kept.
fn replied<'a>(cx: &'a mut TestAppContext, reply: &str) -> (FakeHub, Ui<'a>) {
    replied_with(cx, reply, TWO_SECONDS)
}

/// The same, with the kept copy holding `video`.
fn replied_with<'a>(cx: &'a mut TestAppContext, reply: &str, video: &[u8]) -> (FakeHub, Ui<'a>) {
    let mut world = World::default();
    let id = AttachmentId::from("kept-demo".to_string());
    let size_bytes = u64::try_from(video.len()).unwrap();
    let attachment = Attachment { id: id.clone(), name: "demo.webm".into(), media_type: MediaType::Webm, size_bytes };
    let places = reply.matches("](demo.webm)").count();
    let kept = vec![LinkedImage { address: "demo.webm".to_string(), attachment }; places];
    world.attachments.insert(id, video.to_vec());
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let events =
        vec![user_prompt("Show me."), taken(Seq::new(1)), text_pointing("shown", reply, kept), finished_turn(3, 0.01)];
    world.chat(&workspace, SessionStatus::Idle, events);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the video's still", |ui| ui.labelled("Play").len() == 2);
    (fake, ui)
}

/// Draws a frame `STEP` later each time until `done`, as a display would while the video plays.
fn play_until(ui: &mut Ui<'_>, what: &str, done: impl Fn(&mut Ui<'_>) -> bool) {
    ui.wait(what, |ui| {
        ui.play(STEP);
        done(ui)
    });
}

/// The still shows with the play mark over it until it's played, and it plays to its end and stops there, each frame
/// taken from the player's thread as it's due.
#[gpui_kit::test]
fn a_video_a_reply_points_to_plays_where_it_is_to_its_end(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied(cx, "![The demo](demo.webm)");
    assert!(ui.texts_shown().contains("0:00 / 0:02"), "how long it is, from the file");

    ui.click_labelled("Play", 0);

    assert_eq!(ui.labelled("Pause").len(), 1, "playing");
    play_until(&mut ui, "a second in", |ui| ui.texts_shown().contains("0:01 / 0:02"));
    play_until(&mut ui, "its end", |ui| ui.labelled("Play").len() == 1);
    assert!(ui.texts_shown().contains("0:02 / 0:02"), "stopped at its end");
}

/// One that doesn't say how long it is plays to its last frame, and again from its start.
#[gpui_kit::test]
fn a_video_that_doesn_t_say_how_long_it_is_plays_to_its_end_and_again(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied_with(cx, "![The demo](demo.webm)", UNKNOWN_LENGTH);
    assert!(ui.texts_shown().contains("0:00"), "where it is, with no length");

    ui.click_labelled("Play", 0);
    play_until(&mut ui, "its end", |ui| ui.labelled("Play").len() == 1);
    ui.click_labelled("Play", 0);

    assert_eq!(ui.labelled("Pause").len(), 1, "playing again");
    play_until(&mut ui, "its end again", |ui| ui.labelled("Play").len() == 1);
}

/// A video that isn't drawn for a while, such as one scrolled out of sight, stops where it was last seen, rather than
/// playing on unseen.
#[gpui_kit::test]
fn a_video_that_goes_undrawn_stops_where_it_was(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied(cx, "![The demo](demo.webm)");
    ui.click_labelled("Play", 0);
    ui.play(STEP);

    // Its row is drawn again only once it's back in sight, and asked for the frame after the last it was drawn in.
    ui.cx.executor().advance_clock(Duration::from_secs(1));
    ui.bring_the_next_frame();
    ui.frame();

    assert_eq!(ui.labelled("Play").len(), 1, "stopped");
    assert!(ui.texts_shown().contains("0:00 / 0:02"), "where it was last drawn, not a second on");
}

/// Expand opens the same video over the window, carrying on from where it is, with no Expand of its own, and the one
/// in the chat stays.
#[gpui_kit::test]
fn expand_opens_the_video_over_the_window(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied(cx, "![The demo](demo.webm)");

    ui.click_label("Expand");

    assert!(ui.has_dialog());
    assert_eq!(ui.labelled("Expand").len(), 1, "only the one in the chat has Expand");
    assert_eq!(ui.labelled("Play").len(), 4, "the play mark and the bar's of the one over the window too");
    ui.press("escape");
    assert!(!ui.has_dialog());
}
