//! A chat from a hub that reads every event together, so a subagent's work is gathered into its call's row, with
//! another subagent's nested in it, and a command: what that row draws is the chat's text like any other, Markdown and
//! plain, in the order it shows.

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::capability::calls::call::Runs;
use protocol::model::{Seq, SessionStatus};

use super::capture;
use super::worlds::chat_world;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{command, failed, printed, reported, subagent, taken, text, user_prompt, within};

/// A text the `markdown` crate panics on, which the kit's text view ended the app on.
const PANICS: &str = "a\n===\n---\nb\n===";
/// What the outer subagent's command is for, which its line says.
const CHECK: &str = "Check the points";
/// What went wrong with the inner subagent, which its call shows under its line, before what it was asked.
const STOPPED: &str = "The inner one stopped.";

/// A subagent that runs a command and starts another part way through its work, each reporting in its own words, with
/// their events falling between each other's in the log, as work done at once does. The other stops short.
fn a_subagent_s_work_with_another_s_in_it() -> World {
    let agent = |message: &str, call: &str, description: &str, prompt: &str| {
        subagent(message, call, "general-purpose", description, prompt, Runs::Waits)
    };
    let events = vec![
        user_prompt("Have the signals looked at."),
        taken(Seq::new(1)),
        agent("msg-1", "outer", "Look at the signals", "Ask of the outer one."),
        within("outer", text("sub-1", "The outer one starts.")),
        within("outer", command("sub-c", "check", "echo checked", Some(CHECK))),
        within("outer", printed("check", "checked\n")),
        within("outer", agent("sub-2", "inner", "Look at one signal", "Ask of the inner one.")),
        within("inner", text("in-1", "The inner one works.")),
        // In the log before the inner one's last words, and shown after them.
        within("outer", text("sub-3", "The outer one ends.")),
        within("inner", text("in-2", PANICS)),
        within("outer", failed("inner", STOPPED)),
        reported("outer", "The outer one is done."),
        text("msg-2", "Both have looked."),
    ];
    let mut world = chat_world(SessionStatus::Idle, events);
    world.reads_every_event_together = true;
    world
}

/// A call nested in a subagent's work has its texts placed by the order its row makes them in, so what it was asked
/// has to be made before its work, and what went wrong before both, as they're shown. A command and what it printed
/// are among them, and so is a text the Markdown parser panics on, as it's written.
#[test]
fn what_a_call_s_row_nests_is_copied_in_the_order_it_shows_with_a_text_the_parser_panics_on() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let calls = ["Look at the signals", CHECK, "Look at one signal"];

    let copied = capture::copied_whole(a_subagent_s_work_with_another_s_in_it, &calls, &text_system);

    let shown = [
        "Have the signals looked at.",
        "Ask of the outer one.",
        "The outer one starts.",
        "echo checked",
        "checked",
        STOPPED,
        "Ask of the inner one.",
        "The inner one works.",
        PANICS,
        "The outer one ends.",
        "Both have looked.",
    ];
    assert_eq!(copied, shown.join("\n\n"));
}

/// What finding counts in a subagent's work gathered into its call, and in another's nested in that, is what the row
/// draws of it, so every match there can be gone to.
#[test]
fn what_a_call_s_row_nests_is_found_as_it_s_drawn() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let found = capture::found_both_ways(a_subagent_s_work_with_another_s_in_it, &["one"], &text_system).remove(0);

    // The outer one's first words are the chat's fourth event, and the inner one's its eighth, after the command.
    let nested = |event: &str| found.counted.get(event).copied();
    let words = (nested("(Event(Seq(4), 0), Text)"), nested("(Event(Seq(8), 0), Text)"));
    assert_eq!(words, (Some(1), Some(1)), "the outer one's words and the inner one's: {:?}", found.counted);
    assert_eq!(found.drawn, found.counted);
}
