//! Whether GitHub would take a push to a repository as an account, as `git push` finds out before it sends anything.

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use http::HeaderValue;
use protocol::client::ApiError;
use protocol::model::GithubRepositoryName;
use protocol::worker::GitCredential;

use super::client::{Github, not_reached, unavailable, within};

/// What GitHub's git answers a push's first request with when it would take the push.
const RECEIVE_PACK: &str = "application/x-git-receive-pack-advertisement";

/// How much of what GitHub says as it refuses a push is kept as its reason. GitHub says a line, such as `Permission to
/// cli/cli.git denied to ada.`, so this only keeps a body that isn't GitHub's from filling a record.
const REASON_UP_TO: usize = 500;

/// What GitHub said of a push to a repository as an account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PushAnswer {
    /// It would take it.
    Takes,
    /// It refused it, saying why, in its own words.
    Refused(String),
    /// It has no such repository that the account can see.
    NotFound,
    /// It didn't take the account's token at all.
    TokenRefused,
}

impl Github {
    /// What a push to `repository` as `credential` would meet, by what GitHub answers the request `git push` makes
    /// before it sends anything: `GET {repository}.git/info/refs?service=git-receive-pack`, with the username and token
    /// as git's credential helper gives them, which is where GitHub decides whether the push is taken. So it's GitHub's
    /// own word on everything that limits a push but a branch's own rules: the user's access, a fine-grained token's
    /// repositories and permissions, an organization's SAML SSO and its restrictions on OAuth Apps. It sends nothing,
    /// so it changes nothing. Within [`ASK_WITHIN`](super::ASK_WITHIN), whatever GitHub does, and `Unavailable` when
    /// GitHub can't be reached, doesn't answer in time, limits how often it's asked, or answers with anything else.
    pub(crate) async fn push_reach(
        &self,
        repository: &GithubRepositoryName,
        credential: &GitCredential,
    ) -> Result<PushAnswer, ApiError> {
        within(self.ask_push(repository, credential)).await
    }

    async fn ask_push(
        &self,
        repository: &GithubRepositoryName,
        credential: &GitCredential,
    ) -> Result<PushAnswer, ApiError> {
        let client = self.client().await?;
        let uri = format!("{}/{repository}.git/info/refs?service=git-receive-pack", self.config.git);
        let asking = http::Request::get(uri).header(http::header::AUTHORIZATION, basic(credential)?);
        let request = client.build_request(asking, None::<&()>).map_err(|e| not_reached(&e))?;
        let response = client.execute(request).await.map_err(|e| not_reached(&e))?;
        let status = response.status().as_u16();
        let advertises = response.headers().get(http::header::CONTENT_TYPE).is_some_and(|said| {
            said.to_str().is_ok_and(|said| said.split(';').next().is_some_and(|kind| kind.trim() == RECEIVE_PACK))
        });
        match status {
            // Every ref the repository has follows, which can be a lot, and says nothing more of the push.
            200 if advertises => Ok(PushAnswer::Takes),
            200 => Err(unavailable("what answered isn't GitHub's git, as a network that sends its own page has it")),
            401 => Ok(PushAnswer::TokenRefused),
            404 => Ok(PushAnswer::NotFound),
            403 => {
                let said = client.body_to_string(response).await.map_err(|e| not_reached(&e))?;
                Ok(PushAnswer::Refused(reason(&said)))
            }
            429 => Err(unavailable("GitHub is limiting how often the hub asks it; it asks again later")),
            _ => Err(unavailable(format!("GitHub answered {status}"))),
        }
    }
}

/// The header that gives GitHub `credential`'s username and token, as git does, marked sensitive, so nothing that
/// writes out a request, such as a trace of it, writes the token.
fn basic(credential: &GitCredential) -> Result<HeaderValue, ApiError> {
    let GitCredential { username, token, .. } = credential;
    let encoded = BASE64_STANDARD.encode(format!("{username}:{}", token.as_str()));
    let mut header = HeaderValue::from_str(&format!("Basic {encoded}"))
        .map_err(|_| unavailable("the default's username and token can't go in a request"))?;
    header.set_sensitive(true);
    Ok(header)
}

/// GitHub's reason for refusing a push, from what it said, on one line and no longer than [`REASON_UP_TO`], or that it
/// gave none.
fn reason(said: &str) -> String {
    let said = said.split_whitespace().collect::<Vec<_>>().join(" ");
    if said.is_empty() {
        return "GitHub refused it, and said nothing more".to_string();
    }
    match said.char_indices().nth(REASON_UP_TO) {
        Some((cut, _)) => format!("{}…", &said[..cut]),
        None => said,
    }
}
