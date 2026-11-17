//! Whether GitHub takes a machine's own `gh` login's token, which its worker hands the hub.

use protocol::model::{GhLoginWorks, GithubUsername};
use protocol::worker::FoundToken;
use serde::Deserialize;

use super::client::{Github, within};

/// What `GET /user` answers that this reads.
#[derive(Deserialize)]
struct User {
    login: String,
}

impl Github {
    /// Whether GitHub takes `found`, the token of the login `gh` names `username`, as [`GhLoginWorks`] says, by what
    /// `GET /user` answers, within [`ASK_WITHIN`](super::ASK_WITHIN): a success naming `username`, whatever its case,
    /// as GitHub's usernames are, works, and a 401 is refused, unless it's a token `gh` renews by itself, whose time may
    /// only be up. Anything else, or no answer, can't tell.
    pub(crate) async fn found_login_works(&self, found: &FoundToken, username: &GithubUsername) -> GhLoginWorks {
        let asked = within(self.ask(http::Method::GET, "/user", Some(found.token.as_str()), None)).await;
        let Ok(answer) = asked else { return GhLoginWorks::CouldntTell };
        match answer.status {
            _ if answer.limited => GhLoginWorks::CouldntTell,
            200..=299 => match serde_json::from_str::<User>(&answer.body) {
                Ok(user) if user.login.eq_ignore_ascii_case(username.as_str()) => GhLoginWorks::Yes,
                _ => GhLoginWorks::CouldntTell,
            },
            401 if !found.renews => GhLoginWorks::Refused,
            _ => GhLoginWorks::CouldntTell,
        }
    }
}
