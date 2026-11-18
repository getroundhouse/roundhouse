//! What it means, in words for a person, when the hub didn't do what a client asked: why it refused, as it says it, or
//! what went wrong on the way. It's the same for every client. A client says only what's particular to what it asked,
//! such as what a conflict means for a sign-in, and leaves the rest to this.

use protocol::client::ErrorCode;

use crate::ClientError;

/// Why `e` stopped what was asked, to follow "Couldn't …: ". A hub too old to know the request is said to be, since
/// updating it is what helps. Any other refusal is the hub's own message, which is meant for a person, and a failure
/// on the way, such as the connection going, says what went wrong.
pub fn reason(e: &ClientError) -> String {
    let ClientError::Api(api) = e else { return e.to_string() };
    match api.code {
        ErrorCode::Unsupported => "this hub can't do that until it's updated".to_string(),
        ErrorCode::Unauthorized
        | ErrorCode::LocalOnly
        | ErrorCode::InvalidRequest
        | ErrorCode::VersionMismatch
        | ErrorCode::NeedsForce
        | ErrorCode::NameTaken
        | ErrorCode::NotFound
        | ErrorCode::Conflict
        | ErrorCode::Worker
        | ErrorCode::Unavailable
        | ErrorCode::Internal
        | ErrorCode::Unknown => api.message.clone(),
    }
}

/// What to say when the hub couldn't do `what`, such as "disconnect it", for `e`: `Couldn't disconnect it: …`, with
/// the [`reason`].
pub fn couldnt(what: &str, e: &ClientError) -> String {
    format!("Couldn't {what}: {}", reason(e))
}

/// What to say when the hub no longer has the `thing` asked about, such as "account", as once it's gone elsewhere.
pub fn gone(thing: &str) -> String {
    format!("The hub no longer has this {thing}.")
}

#[cfg(test)]
mod tests {
    use protocol::client::ApiError;

    use super::*;

    fn refused(code: ErrorCode) -> ClientError {
        ClientError::Api(ApiError::new(code, "no such account"))
    }

    #[test]
    fn a_hub_too_old_is_said_to_need_updating_any_other_refusal_is_its_own_message_and_a_failure_on_the_way_is_said() {
        let said = |e: ClientError| couldnt("disconnect it", &e);

        assert_eq!(
            said(refused(ErrorCode::Unsupported)),
            "Couldn't disconnect it: this hub can't do that until it's updated"
        );
        assert_eq!(said(refused(ErrorCode::NotFound)), "Couldn't disconnect it: no such account");
        assert_eq!(said(ClientError::Disconnected), "Couldn't disconnect it: the connection to the hub was lost");
    }

    #[test]
    fn what_the_hub_no_longer_has_is_named() {
        assert_eq!(gone("app"), "The hub no longer has this app.");
    }
}
