//! A chat from a hub that reads every event together, so a subagent's work is gathered into its call's row, with
//! another subagent's nested in it: what that row draws is the chat's Markdown like any other, in the order it shows.

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::model::SessionStatus;
use serde_json::json;

use super::capture;
use super::events::{taken, text, tool_result, tool_use, user_prompt, within};
use super::world::World;
use super::worlds::chat_world;

/// A text the `markdown` crate panics on, which the kit's text view ended the app on.
const PANICS: &str = "a\n===\n---\nb\n===";

/// A subagent that starts another part way through its work, each reporting in its own words, with their events
/// falling between each other's in the log, as work done at once does.
fn a_subagent_s_work_with_another_s_in_it() -> World {
    let agent = |description: &str, prompt: &str| {
        json!({
            "description": description,
            "subagent_type": "general-purpose",
            "prompt": prompt,
        })
    };
    let events = vec![
        user_prompt("Have the signals looked at."),
        taken(1),
        tool_use("msg-1", "outer", "Agent", agent("Look at the signals", "Ask of the outer one.")),
        within("outer", text("sub-1", "The outer one starts.")),
        within("outer", tool_use("sub-2", "inner", "Agent", agent("Look at one signal", "Ask of the inner one."))),
        within("inner", text("in-1", "The inner one works.")),
        // In the log before the inner one's last words, and shown after them.
        within("outer", text("sub-3", "The outer one ends.")),
        within("inner", text("in-2", PANICS)),
        within("outer", tool_result("inner", "The inner one is done.")),
        tool_result("outer", "The outer one is done."),
        text("msg-2", "Both have looked."),
    ];
    let mut world = chat_world(SessionStatus::Idle, events);
    world.reads_every_event_together = true;
    world
}

/// A call nested in a subagent's work has its texts placed by the order its row makes them in, so what it was asked
/// has to be made before its work, as it's shown. A text the Markdown parser panics on is among them, as it's written.
#[test]
fn what_a_call_s_row_nests_is_copied_in_the_order_it_shows_with_a_text_the_parser_panics_on() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let calls = ["Look at the signals", "Look at one signal"];

    let copied = capture::copied_whole(a_subagent_s_work_with_another_s_in_it, &calls, &text_system);

    let shown = [
        "Have the signals looked at.",
        "Ask of the outer one.",
        "The outer one starts.",
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

    // The outer one's first words are the chat's fourth event, and the inner one's its sixth.
    let nested = |event: &str| found.counted.get(event).copied();
    let words = (nested("(Event(4, 0), Text)"), nested("(Event(6, 0), Text)"));
    assert_eq!(words, (Some(1), Some(1)), "the outer one's words and the inner one's: {:?}", found.counted);
    assert_eq!(found.drawn, found.counted);
}
