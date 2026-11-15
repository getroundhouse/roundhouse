//! What a person pastes as a token for a login the hub is to hold, of any kind: trimmed, since what's pasted often ends
//! in a line break, and refused when it can't be a token.

use protocol::client::{ApiError, ErrorCode};

/// Longer than any token the hub is given: `claude setup-token`'s is about a hundred characters, and GitHub's at most
/// 255.
const MAX_TOKEN_BYTES: usize = 4096;

/// `pasted` trimmed. Refused with `InvalidRequest` when that's empty, very long, or has a space or a control character
/// inside, which no token has, and which an environment variable or a header can't always carry.
pub(crate) fn pasted_token(pasted: &str) -> Result<&str, ApiError> {
    let token = pasted.trim();
    let refused = |message: &str| Err(ApiError::new(ErrorCode::InvalidRequest, message));
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return refused("the token is empty, or longer than any token is");
    }
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return refused("the token has a space or a line break inside, which no token has");
    }
    Ok(token)
}
