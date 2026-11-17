#![cfg(test)]

use crate::support::{FakeHub, Stack, Ui};
use client_core::fake_hub::World;
use client_core::fake_hub::events::{
    self, advice_failed, advised, advised_encrypted, finished_turn, taken, text, typed_call, user_prompt,
};
use gpui_kit::test::ElementSnapshot;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use image::ImageFormat;
use protocol::capability::calls::Call;
use protocol::event::EventBody;
use protocol::model::{Seq, SessionStatus};
use std::io::Cursor;

/// The fake agent's commands come with no description, so their line says they run something.
pub(crate) const COMMAND_LINE: &str = "Run";
/// Asking the advisor, which the API runs rather than the agent.
const ADVISOR_LINE: &str = "Ask the advisor";

/// One walk through opening a command: it opens to what it printed, closes again, and one that printed nothing says so.
#[gpui_kit::test]
fn a_call_opens_to_what_it_did_and_closes_again(cx: &mut TestAppContext) {
    // The hub records nothing for what `true` printed, where the CLI sends a sentence saying it printed nothing.
    let (_fake, mut ui) =
        open_on_a_turn(cx, [ran(1, "echo printed-by-the-tool", "printed-by-the-tool\n"), ran(2, "true", "")].concat());
    assert_eq!(command_open(&mut ui, 0), Some(false), "a call starts closed");
    assert!(ui.shown("output/").is_empty());

    ui.click_labelled(COMMAND_LINE, 0);

    assert_eq!(command_open(&mut ui, 0), Some(true));
    assert_eq!(ui.shown("command/").len(), 1, "the command");
    assert_eq!(ui.shown("output/").len(), 1, "and what it printed");

    ui.click_labelled(COMMAND_LINE, 0);

    assert_eq!(command_open(&mut ui, 0), Some(false));
    assert!(ui.shown("output/").is_empty());

    ui.click_labelled(COMMAND_LINE, 1);

    assert_eq!(command_open(&mut ui, 1), Some(true), "the command that printed nothing opens");
    assert!(ui.shown("output/").is_empty(), "it printed nothing to show");
    assert_eq!(ui.shown("no-output/").len(), 1, "so it says so");
}

/// As a prompt's does, so a screenshot made smaller to fit the column can be looked at closely. On a real hub, as the
/// one window test of an image a tool sent back reaching the window from where the hub keeps it.
#[gpui_kit::test]
fn a_read_of_an_image_opens_to_the_image_the_hub_kept_which_opens_over_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_reading_an_image(cx, &stack);
    assert!(ui.shown("picture/").is_empty(), "a call starts closed");

    ui.click_labelled("Read 1 image", 0);

    ui.wait("the image, from the hub", |ui| ui.shown("picture/").len() == 1);
    assert!(!ui.has_dialog(), "nothing is over the window until it's clicked");

    let sent_back = ui.shown("picture/").remove(0);
    ui.click(sent_back);

    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "it opens over the window");
    ui.press("escape");
    assert!(!ui.has_dialog(), "and Escape closes it");
}

/// A new workspace where the agent has read `shot.png`, an image, and finished its turn.
fn after_reading_an_image<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let workspace = ui.new_workspace();
    // Wide enough to click as a hand does, which lets go a pixel from where it pressed: its frame takes a click
    // inside its edge, which on an image four pixels wide leaves two.
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(40, 30, image::Rgba([200, 30, 30, 255])).write_to(&mut png, ImageFormat::Png).unwrap();
    std::fs::write(ui.worktree(&workspace).join("shot.png"), png.into_inner()).unwrap();
    ui.input("read:shot.png; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    ui
}

/// A new workspace, open in the app.
pub(crate) fn in_a_new_workspace<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui
}

/// Runs `command`, opens its call as [`open_a_call`] does, and gives the IDs of the block's command and of its output.
pub(crate) fn open_a_command(ui: &mut Ui<'_>, command: &str) -> (ElementId, ElementId) {
    open_a_call(ui, &format!("tool:{command}"), COMMAND_LINE);
    assert_eq!(command_open(ui, 0), Some(true));
    (ui.shown("command/").remove(0), ui.shown("output/").remove(0))
}

/// Runs the fake agent's `step` between enough words that the transcript can scroll either way, and opens the call
/// it makes, whose line says `line`, with the transcript at its end, where the button that jumps to the end would
/// leave it, so the call is on screen however far the transcript had followed the turn.
pub(super) fn open_a_call(ui: &mut Ui<'_>, step: &str, line: &str) {
    ui.input(&format!("burst:3000; {step}; burst:150; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    // The line the turn ends with can come just after, and drawn mid-click it would move the call's line away from
    // under the pointer.
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    to_the_end(ui);
    ui.click_labelled(line, 0);
}

/// A chat held by a fake hub, of one turn in which the agent did what `did` records. It's open at its end, with the
/// turn's end drawn. The hub goes with the test, so it's returned to be kept.
pub(super) fn open_on_a_turn(cx: &mut TestAppContext, did: Vec<EventBody>) -> (FakeHub, Ui<'_>) {
    let mut events = vec![user_prompt("Check the signals."), taken(Seq::new(1))];
    events.extend(did);
    events.push(finished_turn(12, 0.04));
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, events);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    (fake, ui)
}

/// A command the agent ran, as its `n`th call, and what it printed, as the hub records them from Claude.
pub(super) fn ran(n: usize, command: &str, printed: &str) -> Vec<EventBody> {
    let call = format!("toolu-{n}");
    vec![events::command(&format!("msg-{n}"), &call, command, None), events::printed(&call, printed)]
}

/// Whether the `n`th command's line says it's open, as it tells assistive apps.
fn command_open(ui: &mut Ui<'_>, n: usize) -> Option<bool> {
    ui.labelled(COMMAND_LINE).get(n).and_then(ElementSnapshot::expanded)
}

/// One walk through what a call to the advisor opens to, in the order the calls were made: its advice, or a line
/// saying why there's nothing to read.
#[gpui_kit::test]
fn a_call_to_the_advisor_opens_to_its_advice_or_a_line_saying_why_there_is_none(cx: &mut TestAppContext) {
    let advisor = |id: &str| typed_call(&format!("msg-{id}"), id, Call::Advisor);
    let (_fake, mut ui) = open_on_a_turn(
        cx,
        vec![
            advisor("srvtoolu-1"),
            advised_encrypted("msg-srvtoolu-1", "srvtoolu-1"),
            advisor("srvtoolu-2"),
            advised("msg-srvtoolu-2", "srvtoolu-2", "Test it first."),
            advisor("srvtoolu-3"),
            advice_failed("msg-srvtoolu-3", "srvtoolu-3", "overloaded"),
            advisor("srvtoolu-4"),
            EventBody::ToolCallDropped { tool_use_id: "srvtoolu-4".into() },
            advisor("srvtoolu-5"),
            advised("msg-srvtoolu-5", "srvtoolu-5", ""),
            // Last, so the turn ends on it, and nothing went on without it.
            text("msg-before-the-last", "One more thing to ask."),
            advisor("srvtoolu-6"),
        ],
    );
    assert_eq!(ui.labelled(ADVISOR_LINE).len(), 6, "a line for each call, the failed one too");
    let parts = ["encrypted/", "advice/", "dropped/", "empty-reply/", "no-reply/"];
    let shown = |ui: &mut Ui<'_>| parts.map(|part| ui.shown(part).len());
    assert_eq!(shown(&mut ui), [0; 5], "a call starts closed");

    // Each call, opened alone, shows the one part that says what came back for it, if any. The chat is taken to its
    // end each time, since a call opens under its line, which a click keeps still, and the last is at the bottom.
    let cases = [
        (Some("encrypted/"), "a reply only the agent can read"),
        (Some("advice/"), "a reply that can be read"),
        (None, "a call that failed, whose line says so, and opens to nothing more"),
        (Some("dropped/"), "a call the agent went on without, rather than one whose reply might still come"),
        (Some("empty-reply/"), "a reply with nothing in it"),
        (Some("no-reply/"), "a call the turn ended on, with no reply at all"),
    ];
    for (at, (part, case)) in cases.into_iter().enumerate() {
        ui.click_labelled(ADVISOR_LINE, at);
        to_the_end(&mut ui);

        assert_eq!(shown(&mut ui), parts.map(|shows| usize::from(Some(shows) == part)), "{case}, of {parts:?}");
        ui.click_labelled(ADVISOR_LINE, at);
    }
}

/// Takes the chat to its end, where the button that jumps there would leave it.
pub(super) fn to_the_end(ui: &mut Ui<'_>) {
    ui.cx.update_entity(&ui.transcript().unwrap(), |t, cx| {
        t.list().go_to_latest();
        cx.notify();
    });
}
