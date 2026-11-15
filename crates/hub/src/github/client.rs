use std::time::Duration;

use octocrab::Octocrab;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{GithubToken, GithubUser, GithubUserId};
use serde::Deserialize;
use tokio::sync::OnceCell;
use tracing::warn;

use super::expiration::ends_at_ms;

/// How long a call to GitHub has, from asking to the end of its answer. GitHub answers in well under a second, so this
/// leaves room for a slow network and a TLS handshake, and doesn't leave a client waiting long on a GitHub it can't
/// reach.
pub(crate) const ASK_WITHIN: Duration = Duration::from_secs(15);

/// The header GitHub puts on an answer to a call with a token that ends, saying when.
const EXPIRATION: &str = "github-authentication-token-expiration";

/// GitHub's API, as the hub calls it. Only the hub calls GitHub, so its address, its answers and the time limit on each
/// call are known here and nowhere else.
pub(crate) struct Github {
    /// Such as `https://api.github.com`, as `main` gives it, or a test's stand-in.
    api: String,
    /// Made on the first call, on the runtime that runs it, since Octocrab starts a task as it's made. Making it also
    /// loads the system's certificates, which only a call needs, so a machine without them fails only its calls.
    client: OnceCell<Octocrab>,
}

/// Who GitHub says a token is, and when it ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenOwner {
    pub(crate) user: GithubUser,
    /// By GitHub's word, as [`HeldGithubLogin::ends_at_ms`](protocol::model::HeldGithubLogin::ends_at_ms) says.
    pub(crate) ends_at_ms: Option<i64>,
}

/// What `GET /user` answers that the hub reads. GitHub calls the user's name its `login`.
#[derive(Deserialize)]
struct User {
    id: u64,
    login: String,
}

/// What GitHub answered, as far as the hub reads it.
struct Answered {
    status: u16,
    /// Whether it says GitHub is limiting how often the hub calls, rather than refusing the token: a 429, or a 403 that
    /// says when to call again or that none of the hub's calls are left.
    limited: bool,
    expiration: Option<String>,
    body: String,
}

impl Github {
    pub(crate) fn new(api: String) -> Self {
        Self { api, client: OnceCell::new() }
    }

    /// Who `token` is and when it ends, as `GET /user` says, within [`ASK_WITHIN`], whatever GitHub does. Refused with
    /// `InvalidRequest` when GitHub doesn't take the token as a user's, as with one that has ended or been revoked, and
    /// with `Unavailable` when GitHub can't be reached, doesn't answer in time, is limiting the hub's calls, or answers
    /// with anything else.
    pub(crate) async fn token_owner(&self, token: &GithubToken) -> Result<TokenOwner, ApiError> {
        match tokio::time::timeout(ASK_WITHIN, self.ask_who(token)).await {
            Ok(answer) => owner(answer?),
            Err(_) => Err(unavailable(format!("GitHub didn't answer within {} s", ASK_WITHIN.as_secs()))),
        }
    }

    async fn ask_who(&self, token: &GithubToken) -> Result<Answered, ApiError> {
        let client = self.client().await?.user_access_token(token.as_str().to_string()).map_err(|e| not_reached(&e))?;
        let response = client._get("/user").await.map_err(|e| not_reached(&e))?;
        let status = response.status().as_u16();
        let header =
            |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_string);
        let calls_left = header("x-ratelimit-remaining");
        let limited =
            status == 429 || (status == 403 && (header("retry-after").is_some() || calls_left.as_deref() == Some("0")));
        let expiration = header(EXPIRATION);
        let body = client.body_to_string(response).await.map_err(|e| not_reached(&e))?;
        Ok(Answered { status, limited, expiration, body })
    }

    async fn client(&self) -> Result<&Octocrab, ApiError> {
        self.client
            .get_or_try_init(|| async {
                let builder = Octocrab::builder().base_uri(self.api.as_str()).map_err(|e| not_reached(&e))?;
                builder.build().map_err(|e| not_reached(&e))
            })
            .await
    }
}

/// Who the token is, from what GitHub answered.
fn owner(answer: Answered) -> Result<TokenOwner, ApiError> {
    let Answered { status, limited, expiration, body } = answer;
    if limited {
        return Err(unavailable("GitHub is limiting how often the hub calls it; try again in a minute"));
    }
    if matches!(status, 401 | 403) {
        let message = "GitHub doesn't take that token as a user's, as when it has ended or been revoked";
        return Err(ApiError::new(ErrorCode::InvalidRequest, message));
    }
    if !(200..300).contains(&status) {
        return Err(unavailable(format!("GitHub answered {status}")));
    }
    let user: User = serde_json::from_str(&body)
        .map_err(|e| unavailable(format!("what GitHub answered isn't a user, as `GET /user` gives: {e}")))?;
    let ends_at_ms = expiration.and_then(|said| {
        let read = ends_at_ms(&said);
        if read.is_none() {
            warn!(said, "couldn't read when GitHub says a token ends, so its record says it doesn't");
        }
        read
    });
    Ok(TokenOwner { user: GithubUser { id: GithubUserId::new(user.id), name: user.login.into() }, ends_at_ms })
}

fn not_reached(e: &octocrab::Error) -> ApiError {
    unavailable(format!("couldn't reach GitHub: {}", error_chain(e)))
}

fn unavailable(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Unavailable, message)
}
