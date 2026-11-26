use url::form_urlencoded;

use crate::{AuthorizationCode, OauthError, SignInState};

/// What an authorization server sent the person back with, unchecked.
#[derive(Debug, Default)]
pub struct Returned {
    pub(super) code: Option<AuthorizationCode>,
    pub(super) state: Option<SignInState>,
    pub(super) iss: Option<String>,
    pub(super) error: Option<OauthError>,
    /// Whether one of them was there twice, which RFC 6749, section 3.1, has no server send.
    pub(super) repeated: bool,
}

impl Returned {
    /// What the query of the address they were sent back to says, such as `code=…&state=…`.
    pub fn of_query(query: &str) -> Self {
        let mut returned = Returned::default();
        for (name, value) in form_urlencoded::parse(query.as_bytes()) {
            let repeated = match &*name {
                "code" => returned.code.replace(AuthorizationCode::new(value)).is_some(),
                "state" => returned.state.replace(SignInState::new(value)).is_some(),
                "iss" => returned.iss.replace(value.into_owned()).is_some(),
                "error" => returned.error.replace(OauthError::new(&value)).is_some(),
                _ => false,
            };
            returned.repeated |= repeated;
        }
        returned
    }
}
