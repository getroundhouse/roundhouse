//! Videos: one a reply points to, which plays where it is, and one the file tab opens from the worktree, which plays
//! there if it's a WebM the app plays, and otherwise says what it is, for the system's player. The worker keeps a
//! reply's videos and reads the worktree's, and how the window plays them is the app's, so they're played on a fake
//! hub holding made-up replies and files, with no worker to wait for. Each frame is decoded on the player's own thread,
//! by the system's clock, while the window takes each as it's due by the test's, which the test moves.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, taken, text_pointing, user_prompt};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::LinkedImage;
use protocol::model::{AttachmentId, FileContents, Seq, SessionStatus, WorktreeFiles};
use video::fixtures::{AV1, MP4, TEN_BIT, TWO_SECONDS, UNKNOWN_LENGTH};

use crate::support::{FakeHub, Ui};
use desktop::file_view::opened_with_system;

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

/// The window on a fake hub on this machine holding a workspace at `/workspaces/ledger`, whose worker is on the hub's
/// machine, whose worktree has `path`, of `media_type`, holding `bytes`, opened from All files. The hub goes with the
/// test, so it's returned to be kept.
fn opened<'a>(cx: &'a mut TestAppContext, path: &str, media_type: MediaType, bytes: &[u8]) -> (FakeHub, Ui<'a>) {
    opened_by(cx, path, media_type, bytes, "local")
}

/// The same, with the workspace's worker run by `provider`.
fn opened_by<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
    media_type: MediaType,
    bytes: &[u8],
    provider: &str,
) -> (FakeHub, Ui<'a>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.workspace_mut(&workspace).provider = provider.into();
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    world.worktree = WorktreeFiles { paths: vec![path.to_string()], folders: Vec::new(), truncated: false };
    let contents = FileContents::Video { media_type, bytes: u64::try_from(bytes.len()).unwrap() };
    world.worktree_files.insert(path.to_string(), contents);
    world.worktree_videos.insert(path.to_string(), bytes.to_vec());
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.view().selected() == Some(&workspace) && ui.pane().is_some());
    ui.click("sidebar-tab-all-files");
    ui.wait("All files to list it", |ui| ui.has(format!("all-file-{path}")));
    ui.click(format!("all-file-{path}"));
    ui.wait("the video", |ui| shown(ui).is_some());
    (fake, ui)
}

/// What the file tab says of the video it shows, and whether it plays it there.
fn shown(ui: &Ui<'_>) -> Option<(String, bool)> {
    ui.cx.read_entity(&viewer(ui), |v, _| v.shown_video())
}

fn viewer(ui: &Ui<'_>) -> gpui_kit::Entity<desktop::file_view::FileView> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.viewer.clone())).unwrap()
}

/// The time of the frame the tab's video draws, `None` while it draws its still, and whether it keeps a decoder.
fn drawn(ui: &Ui<'_>) -> (Option<Duration>, bool) {
    let player = ui.cx.read_entity(&viewer(ui), |v, _| v.video_player()).expect("a video playing in the tab");
    ui.cx.read_entity(&player, |p, _| (p.frame_drawn(), p.decodes()))
}

/// A WebM plays in the tab, which says what kind it is, how big and how long, read from its first frame, drawing each
/// frame as it's due, to its last, after which it lets go of its decoder.
#[gpui_kit::test]
fn a_webm_opened_from_all_files_plays_in_the_tab(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo.webm", MediaType::Webm, TWO_SECONDS);

    let (said, here) = shown(&ui).unwrap();
    assert!(here, "it plays in the tab");
    assert!(said.starts_with("WebM · 64 × 36 · 0:02 · "), "{said}");
    assert_eq!(drawn(&ui), (None, false), "its still");
    ui.click_labelled("Play", 0);
    play_until(&mut ui, "a second in", |ui| ui.texts_shown().contains("0:01 / 0:02"));
    let (frame, decodes) = drawn(&ui);
    assert!(frame.is_some_and(|time| time >= Duration::from_millis(900)), "a frame a second in: {frame:?}");
    assert!(decodes);
    play_until(&mut ui, "its end", |ui| ui.labelled("Play").len() == 1);
    assert_eq!(drawn(&ui), (Some(Duration::from_millis(1900)), false), "its last frame, and no decoder");
}

/// Paused, a video keeps its decoder for a while, so it plays on at once, and then lets it go, and played after that,
/// it carries on from where it was.
#[gpui_kit::test]
fn a_paused_video_lets_go_of_its_decoder_after_a_while(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo.webm", MediaType::Webm, TWO_SECONDS);
    ui.click_labelled("Play", 0);
    play_until(&mut ui, "a second in", |ui| ui.texts_shown().contains("0:01 / 0:02"));
    ui.click_label("Pause");
    let paused = drawn(&ui);

    ui.cx.executor().advance_clock(Duration::from_secs(5));
    ui.frame();
    let a_while_on = drawn(&ui);
    ui.cx.executor().advance_clock(Duration::from_secs(6));
    ui.frame();
    let later = drawn(&ui);

    assert!(paused.1 && a_while_on.1, "kept for a while");
    assert_eq!(later, (paused.0, false), "let go of, the frame it was paused on still drawn");
    ui.click_labelled("Play", 0);
    play_until(&mut ui, "its end", |ui| ui.labelled("Play").len() == 1);
    assert!(ui.texts_shown().contains("0:02 / 0:02"));
}

/// An MP4 says what it says of itself, from its `moov` box, which this one has at its end, and plays in the system's
/// player, as does a WebM the app can't play.
#[gpui_kit::test]
fn a_video_the_app_doesn_t_play_says_what_it_is_for_the_system_s_player(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo.mp4", MediaType::Mp4, MP4);

    let (said, here) = shown(&ui).unwrap();
    assert!(!here);
    assert!(said.starts_with("MP4 · 64 × 36 · 0:01 · "), "{said}");
    assert_eq!(ui.labelled("Open in system player").len(), 1);
}

/// One on this machine, named for its kind, opens where it is.
#[gpui_kit::test]
fn a_video_on_this_machine_named_for_its_kind_opens_where_it_is(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo.mp4", MediaType::Mp4, MP4);

    ui.click_label("Open in system player");

    let opened = ui.cx.update(|cx| opened_with_system(cx));
    assert_eq!(opened, [std::path::PathBuf::from("/workspaces/ledger/demo.mp4")]);
}

/// One whose worker is on another machine is opened as a copy, though the hub's on this one.
#[gpui_kit::test]
fn a_video_on_another_machine_opens_as_a_copy(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened_by(cx, "demo.mp4", MediaType::Mp4, MP4, "cloud");

    ui.click_label("Open in system player");

    ui.wait("the copy, opened", |ui| ui.cx.update(|cx| !opened_with_system(cx).is_empty()));
    let opened = ui.cx.update(|cx| opened_with_system(cx));
    assert_ne!(opened, [std::path::PathBuf::from("/workspaces/ledger/demo.mp4")]);
    assert_eq!(std::fs::read(&opened[0]).unwrap(), MP4);
}

/// One with no name the system would know it by is opened as a copy, read from the hub and named for its kind, since
/// the system picks what opens a file by its name.
#[gpui_kit::test]
fn a_video_named_for_no_kind_opens_as_a_copy_named_for_its_kind(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo", MediaType::Mp4, MP4);

    ui.click_label("Open in system player");

    ui.wait("the copy, opened", |ui| ui.cx.update(|cx| !opened_with_system(cx).is_empty()));
    let opened = ui.cx.update(|cx| opened_with_system(cx));
    let [copy] = opened.as_slice() else { panic!("one opened: {opened:?}") };
    assert_eq!(copy.extension().and_then(|e| e.to_str()), Some("mp4"));
    assert_eq!(std::fs::read(copy).unwrap(), MP4, "the whole of it");
}

/// One in a codec the app doesn't decode is for the system's player.
#[gpui_kit::test]
fn a_webm_the_app_can_t_play_is_for_the_system_s_player(cx: &mut TestAppContext) {
    let (_hub, ui) = opened(cx, "demo.webm", MediaType::Webm, AV1);

    let (said, here) = shown(&ui).unwrap();
    assert!(!here, "AV1 isn't decoded");
    assert!(said.starts_with("WebM · "), "{said}");
}

/// So is one in VP9's 10 bits, which opens and whose first frame doesn't decode.
#[gpui_kit::test]
fn a_webm_whose_frames_don_t_decode_is_for_the_system_s_player(cx: &mut TestAppContext) {
    let (_hub, ui) = opened(cx, "demo.webm", MediaType::Webm, TEN_BIT);

    let (said, here) = shown(&ui).unwrap();
    assert!(!here, "10 bits aren't decoded");
    assert!(said.starts_with("WebM · "), "{said}");
}

/// A video playing in the tab as the app connects again stops where it was, and lets go of its decoder, once the new
/// connection has caught up, since what it was reading went with the last.
#[gpui_kit::test]
fn a_video_playing_in_the_tab_stops_where_it_was_on_a_new_connection(cx: &mut TestAppContext) {
    let (_hub, mut ui) = opened(cx, "demo.webm", MediaType::Webm, TWO_SECONDS);
    ui.click_labelled("Play", 0);
    play_until(&mut ui, "a second in", |ui| ui.texts_shown().contains("0:01 / 0:02"));

    crate::catching_up::connect_again(&mut ui);
    ui.wait("the new connection to catch up", |ui| {
        matches!(ui.view().connection(), desktop::app::Connection::Ready(_)) && ui.labelled("Play").len() == 1
    });

    let (frame, decodes) = drawn(&ui);
    assert!(frame.is_some_and(|time| time < Duration::from_millis(1900)), "stopped short of its end: {frame:?}");
    assert!(!decodes, "and let go of its decoder");
}
