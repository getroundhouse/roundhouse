use std::time::Duration;

use octocrab::Octocrab;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{GithubToken, GithubUser, GithubUserId};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tracing::warn;

use super::GithubConfig;
use super::expiration::ends_at_ms;
use crate::GithubProfile;

/// How long a call to GitHub has, from asking to the end of its answer. GitHub answers in well under a second, so this
/// leaves room for a slow network and a TLS handshake, and doesn't leave a client waiting long on a GitHub it can't
/// reach.
pub const ASK_WITHIN: Duration = Duration::from_secs(15);

/// The header GitHub puts on an answer to a call with a token that ends, saying when.
const EXPIRATION: &str = "github-authentication-token-expiration";

/// GitHub's API and its device flow, as the hub calls them. Only the hub calls GitHub, so its addresses, its answers
/// and the time limit on each call are known here and nowhere else.
pub struct Github {
    /// Where it calls, as `main` gives it, or a test's stand-in.
    pub(super) config: GithubConfig,
    /// Made on the first call, on the runtime that runs it, since Octocrab starts a task as it's made. Making it also
    /// loads the system's certificates, which only a call needs, so a machine without them fails only its calls.
    client: OnceCell<Octocrab>,
}

/// Who GitHub says a token is, what its profile says that its commit identity is made from, and when the token ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenOwner {
    pub user: GithubUser,
    pub profile: GithubProfile,
    /// By GitHub's word, as [`HeldGithubLogin::ends_at_ms`](protocol::model::HeldGithubLogin::ends_at_ms) says.
    pub ends_at_ms: Option<i64>,
}

/// What `GET /user` answers that the hub reads. GitHub calls the user's name its `login`, and the name it goes by its
/// `name`, and gives `email` only as the one its profile shows everyone, whatever the token may read, each as `null`
/// for a user who hasn't set it.
#[derive(Deserialize)]
struct User {
    id: u64,
    login: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// What GitHub's API answered, as far as the hub reads it.
pub(super) struct Answered {
    pub(super) status: u16,
    /// Whether it says GitHub is limiting how often the hub calls, rather than refusing the token: a 429, or a 403 that
    /// says when to call again or that none of the hub's calls are left.
    pub(super) limited: bool,
    expiration: Option<String>,
    pub(super) body: String,
}

impl Answered {
    /// Whether GitHub did as it was asked.
    pub(super) fn succeeded(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

impl Github {
    pub fn new(config: GithubConfig) -> Self {
        Self { config, client: OnceCell::new() }
    }

    /// GitHub's page for the OAuth App people sign in through, as [`GithubConfig::app_page`] says.
    pub fn app_page(&self) -> String {
        self.config.app_page()
    }

    /// Who `token` is, its user's name and public email, and when it ends, as `GET /user` says, within [`ASK_WITHIN`],
    /// whatever GitHub does. A name or an email that's only blank is none. Refused with `InvalidRequest` when GitHub
    /// doesn't take the token as a user's, as with one that has ended or been revoked, and with `Unavailable` when
    /// GitHub can't be reached, doesn't answer in time, is limiting the hub's calls, or answers with anything else.
    pub async fn token_owner(&self, token: &GithubToken) -> Result<TokenOwner, ApiError> {
        owner(within(self.ask(http::Method::GET, "/user", Some(token.as_str()), None)).await?)
    }

    /// Asks GitHub's API to `method` `path`, as whoever `bearer` is, if anyone, with `body` as JSON, and answers with
    /// what GitHub answered, whatever it is. Unbounded, so its caller puts its own limit on the whole call.
    pub(super) async fn ask(
        &self,
        method: http::Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<Answered, ApiError> {
        let anyone = self.client().await?;
        let bearing;
        let client = match bearer {
            Some(bearer) => {
                bearing = anyone.user_access_token(bearer.to_string()).map_err(|e| not_reached(&e))?;
                &bearing
            }
            None => anyone,
        };
        let request = client.build_request(http::Request::builder().method(method).uri(path), body);
        let response = client.execute(request.map_err(|e| not_reached(&e))?).await.map_err(|e| not_reached(&e))?;
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

    /// Posts `body` as JSON to `path` of where GitHub's device flow is, asking for JSON back, as the device flow
    /// answers otherwise in a form's encoding, and answers with the status and the body, whatever they are. Unbounded,
    /// so its caller puts its own limit on the whole call.
    pub(super) async fn post_to_login(&self, path: &str, body: &impl Serialize) -> Result<(u16, String), ApiError> {
        let client = self.client().await?;
        let uri = format!("{}{path}", self.config.login);
        let asking = http::Request::post(uri).header(http::header::ACCEPT, "application/json");
        let request = client.build_request(asking, Some(body)).map_err(|e| not_reached(&e))?;
        let response = client.execute(request).await.map_err(|e| not_reached(&e))?;
        let status = response.status().as_u16();
        let body = client.body_to_string(response).await.map_err(|e| not_reached(&e))?;
        Ok((status, body))
    }

    pub(super) async fn client(&self) -> Result<&Octocrab, ApiError> {
        self.client
            .get_or_try_init(|| async {
                let builder = Octocrab::builder().base_uri(self.config.api.as_str()).map_err(|e| not_reached(&e))?;
                builder.build().map_err(|e| not_reached(&e))
            })
            .await
    }
}

/// Who the token is, from what GitHub answered.
fn owner(answer: Answered) -> Result<TokenOwner, ApiError> {
    let Answered { status, limited, expiration, body } = answer;
    if limited {
        return Err(limited_now());
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
    let User { id, login, name, email } = user;
    let said = |text: Option<String>| text.map(|text| text.trim().to_string()).filter(|text| !text.is_empty());
    let profile = GithubProfile { name: said(name), email: said(email) };
    Ok(TokenOwner { user: GithubUser { id: GithubUserId::new(id), name: login.into() }, profile, ends_at_ms })
}

/// What `call` comes to within [`ASK_WITHIN`], from asking to the end of its answer, whatever stage it hangs at, or
/// `Unavailable` once that's up.
pub(super) async fn within<T>(call: impl Future<Output = Result<T, ApiError>>) -> Result<T, ApiError> {
    match tokio::time::timeout(ASK_WITHIN, call).await {
        Ok(answer) => answer,
        Err(_) => Err(unavailable(format!("GitHub didn't answer within {} s", ASK_WITHIN.as_secs()))),
    }
}

/// `Unavailable`, as GitHub limits how often the hub calls it.
pub(super) fn limited_now() -> ApiError {
    unavailable("GitHub is limiting how often the hub calls it; try again in a minute")
}

pub(super) fn not_reached(e: &octocrab::Error) -> ApiError {
    unavailable(format!("couldn't reach GitHub: {}", error_chain(e)))
}

pub(super) fn unavailable(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Unavailable, message)
}
