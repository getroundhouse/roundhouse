//! A chat whose reply points to an image in the worktree, for the screen about it: a board of signals that showed the
//! wrong light for a signal at stop.

#![cfg(test)]

use protocol::attachment::Attachment;
use protocol::event::{EventBody, LinkedImage};

use super::events::{replied, taken, text_pointing, user_prompt};

/// Where the reply in [`pointed`] says its screenshot is.
pub(crate) const AFTER: &str = "docs/screens/board-after.png";

/// The agent fixing the board, and pointing to its screenshot of it after, in the worktree, which the hub kept as
/// `screenshot`.
pub(crate) fn pointed(screenshot: Attachment) -> Vec<EventBody> {
    let reply = format!(
        "The board reads each signal's own aspect now, so the fourth shows red while it's at stop.\n\n\
         ![The board after the fix]({AFTER})\n\nThe screenshot is in `docs/screens/`, beside the design."
    );
    vec![
        user_prompt("Fix the board so a signal at stop shows red, and show me how it looks after."),
        taken(1),
        text_pointing("msg-1", &reply, vec![LinkedImage { address: AFTER.to_string(), attachment: screenshot }]),
        replied(&reply, 9, 0.05),
    ]
}
