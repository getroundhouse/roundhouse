//! What a sign-in that came to nothing says, where that's this device's to say. What the hub refused is worded by
//! `client_core::tool_sign_ins`.

use std::io;
use std::time::Duration;

/// There's no hub to ask.
pub(super) const NO_HUB: &str = "There's no hub to ask.";

/// The hub gave a page the browser isn't opened on.
pub(super) const NO_PAGE: &str = "Couldn't sign in: its page has an address Roundhouse doesn't open.";

/// Nothing could listen for the person to be sent back.
pub(super) fn couldnt_listen(e: &io::Error) -> String {
    format!("Couldn't listen for your browser: {e}")
}

/// Nobody was sent back for as long as a sign-in waits.
pub(super) fn timed_out(waited: Duration) -> String {
    format!("It wasn't approved within {} minutes, so the sign-in ended.", waited.as_secs() / 60)
}
