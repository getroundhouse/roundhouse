//! What the app says when the hub didn't do what the person asked.

use client_core::ClientError;

/// What to tell someone the hub couldn't do `doing`, such as "move the chat", for `e`. A hub older than the app
/// refuses a request it doesn't know, which updating it fixes, so that's said rather than the refusal.
pub(crate) fn couldnt(doing: &str, e: &ClientError) -> String {
    if e.is_unsupported() {
        return format!("couldn't {doing}: the hub is older than this app");
    }
    format!("couldn't {doing}: {e}")
}

#[cfg(test)]
mod tests {
    use protocol::client::{ApiError, ErrorCode};

    use super::*;

    #[test]
    fn a_hub_that_doesnt_know_the_request_is_said_to_be_older_and_any_other_refusal_is_given() {
        let refused = |code| ClientError::Api(ApiError::new(code, "no such chat"));

        assert_eq!(
            couldnt("move the chat", &refused(ErrorCode::Unsupported)),
            "couldn't move the chat: the hub is older than this app"
        );
        assert!(couldnt("move the chat", &refused(ErrorCode::NotFound)).ends_with("no such chat"));
    }
}
