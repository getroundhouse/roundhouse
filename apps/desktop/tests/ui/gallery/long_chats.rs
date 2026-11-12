//! A chat long enough to have an outline worth using: twenty turns, about a change this repo might make, some asked in
//! a few words and some at more length.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::{Seq, SessionStatus, seq_index};

use super::events::{replied, taken, text, user_prompt};
use super::world::World;

/// What each turn was asked, and what the agent said.
pub(crate) const TURNS: [(&str, &str); 20] = [
    (
        "The sidebar should show each workspace's PR state, so I can see at a glance which ones are waiting on review. \
         Where would that come from?",
        "The hub can ask GitHub for each branch and put the answer on `Workspace`, so every client gets it.",
    ),
    ("Go with the hub asking. How often should it ask?", "Every five minutes, and as soon as a branch is pushed."),
    ("Fine. What happens when gh isn't installed?", "Each call fails, so every row would show an error."),
    ("Show nothing then, rather than an error on every row.", "Done: no `gh` means no pull request, quietly."),
    ("Add the field to Workspace and fill it in from the hub.", "Added `pull_request`, filled in on each check."),
    (
        "The tests fail on Linux because gh is missing there too, and CI doesn't install it. Can the stand-in cover it, so \
         nothing needs the real one?",
        "Yes. The stand-in answers for `gh`, so the tests no longer need it installed.",
    ),
    ("Use the stand-in in the hub's tests as well.", "They use it now, and pass on both."),
    ("Now draw it in the sidebar: the fan's colour, as look.md says.", "Each row's fan takes its PR's colour."),
    (
        "The merged purple is too bright next to the others, and it pulls the eye away from the open ones. Try a step \
         darker.",
        "Darker by one step.",
    ),
    ("Better. What about a draft?", "A draft shows the lighter grey, as GitHub does."),
    (
        "Check the draft grey against the faint text, so it doesn't read as disabled. Put them side by side in the gallery \
         if that's easier.",
        "It's lighter than the faint text, so it reads as a state.",
    ),
    ("Run the gallery and show me what changed.", "Four sidebar screens changed, and the conversation."),
    (
        "Only the sidebar screens should change. Why did the conversation one move? Nothing about the chat was meant to be \
         different.",
        "Its tab's mark took the PR's colour too, so it changed with them.",
    ),
    ("Leave the tab alone, then, and accept the sidebar ones.", "The tab is as it was, and those are accepted."),
    ("Write the rule for the fan's colour in sidebar.md, marked Unconfirmed.", "Written, with a line per state."),
    ("Shorter. One sentence for what each colour means is enough.", "Cut to one sentence a colour."),
    (
        "Run the full checks before you push, including clippy with warnings as errors, and the gallery.",
        "Clippy wants a reason on one `expect`. Everything else passes.",
    ),
    ("Say why the cast fits, then.", "The reason says a row count always fits in `u32`."),
    ("Open the PR, and put the screens in its description.", "Opened, with the four screens."),
    ("Is CI happy?", "The checks passed, and it's ready for review."),
];

/// Events each turn takes: its prompt, the agent taking it, the reply, and the turn's end.
const EVENTS_A_TURN: usize = 4;

/// The fourteenth prompt's row in the outline's list, which is the row of the event that took it, where it's drawn. The
/// list shows it when it opens at the end of the chat, and it has more than a screen of chat above it, so it can come
/// to the bottom.
pub(crate) const FOURTEENTH_PROMPT: &str = "outline-prompt/53";

/// The twenty turns, in order.
pub(crate) fn many_turns() -> Vec<EventBody> {
    let mut events = Vec::new();
    for (turn, (asked, said)) in (1..).zip(TURNS) {
        let prompt: Seq = (EVENTS_A_TURN * (turn - 1) + 1).try_into().unwrap();
        events.extend([user_prompt(asked), taken(prompt), text(&format!("msg-{turn}"), said), replied(said, 12, 0.04)]);
    }
    events
}

#[test]
fn the_fourteenth_prompt_is_in_the_row_named() {
    let mut world = World::default();
    let chat = world.chat(&"w".into(), SessionStatus::Idle, many_turns());

    let mut prompts = world.events_from(&chat, 1).filter(|event| matches!(event.body, EventBody::UserPrompt { .. }));
    let fourteenth = prompts.nth(13).unwrap();

    // Taken straight after it's recorded, so in the row after its own.
    assert_eq!(FOURTEENTH_PROMPT, format!("outline-prompt/{}", seq_index(fourteenth.seq)));
}
