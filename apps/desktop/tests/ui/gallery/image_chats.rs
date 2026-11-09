//! A chat whose reply points to an image in the worktree, for the screen about it: a board of signals that showed the
//! wrong light for a signal at stop.

#![cfg(test)]

use protocol::event::EventBody;

use super::events::{finished_turn, taken, text, user_prompt};

/// Where the reply in [`pointed`] says its screenshot is.
pub(crate) const AFTER: &str = "docs/screens/board-after.png";

/// The agent fixing the board, and pointing to its screenshot of it after, in the worktree.
pub(crate) fn pointed() -> Vec<EventBody> {
    vec![
        user_prompt("Fix the board so a signal at stop shows red, and show me how it looks after."),
        taken(1),
        text(
            "msg-1",
            &format!(
                "The board reads each signal's own aspect now, so the fourth shows red while it's at stop.\n\n\
                 ![The board after the fix]({AFTER})\n\nThe screenshot is in `docs/screens/`, beside the design."
            ),
        ),
        finished_turn(9, 0.05),
    ]
}
