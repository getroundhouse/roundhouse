use protocol::client::github_apps::GithubManifestCode;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAppId, GithubAppSlug, GithubUser, GithubUserId};
use serde::Deserialize;

use super::super::Github;
use super::super::client::{Answered, limited_now, unavailable, within};
use super::GithubAppKey;

/// An app GitHub made from its manifest, as it answered the hub's ask for it, with its key.
#[derive(Debug)]
pub(crate) struct MadeApp {
    pub(crate) id: GithubAppId,
    pub(crate) slug: GithubAppSlug,
    pub(crate) name: String,
    pub(crate) owner: GithubUser,
    pub(crate) key: GithubAppKey,
}

/// What `POST /app-manifests/{code}/conversions` answers that the hub reads. GitHub gives a client ID and secret, and a
/// webhook's secret, too, which the hub has no use for, as nothing signs a person in as the app or sends it hooks, so
/// it never reads them, and GitHub keeps them for the person on the app's page.
#[derive(Deserialize)]
struct Made {
    id: u64,
    slug: String,
    name: String,
    owner: Account,
    pem: String,
}

/// A user, or an organization, as GitHub's API gives one.
#[derive(Deserialize)]
pub(super) struct Account {
    pub(super) id: u64,
    pub(super) login: String,
}

impl Account {
    pub(super) fn user(self) -> GithubUser {
        GithubUser { id: GithubUserId::new(self.id), name: self.login.into() }
    }
}

impl Github {
    /// Has GitHub make the app the person made from its manifest into its key, with `code`, which GitHub sent them back
    /// with, within [`ASK_WITHIN`](super::super::ASK_WITHIN). GitHub takes each code once, within an hour, and nobody
    /// asks as anyone. Refused with `InvalidRequest` when GitHub doesn't know the code, as once it's been used or has
    /// ended, and with `Unavailable` when GitHub can't be reached, doesn't answer in time, or answers with anything
    /// but an app, whose answer is never said, since it holds the key.
    pub(crate) async fn make_app(&self, code: &GithubManifestCode) -> Result<MadeApp, ApiError> {
        let path = format!("/app-manifests/{}/conversions", path_part(code.as_str()));
        let answered = within(self.ask(http::Method::POST, &path, None, None)).await?;
        let Answered { status, limited, body, .. } = &answered;
        if *limited {
            return Err(limited_now());
        }
        if matches!(status, 404 | 422) {
            let message = "GitHub doesn't know that code, as once it has been used or has ended";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        if !answered.succeeded() {
            return Err(unavailable(format!("GitHub answered {status} when asked to make the app")));
        }
        let made: Made = serde_json::from_str(body)
            .map_err(|_| unavailable("what GitHub answered when asked to make the app isn't an app"))?;
        Ok(MadeApp {
            id: GithubAppId::new(made.id),
            slug: made.slug.into(),
            name: made.name,
            owner: made.owner.user(),
            key: GithubAppKey::new(made.pem),
        })
    }

    /// The app `slug`'s bot, `{slug}[bot]`, as `GET /users/{slug}[bot]` gives it, asked as nobody, within
    /// [`ASK_WITHIN`](super::super::ASK_WITHIN), since its number makes the address GitHub links its commits to.
    /// Refused with `Unavailable` for whatever GitHub answers but the bot, as before it has made it.
    pub(crate) async fn bot(&self, slug: &GithubAppSlug) -> Result<GithubUser, ApiError> {
        let path = format!("/users/{}%5Bbot%5D", path_part(slug.as_str()));
        let answered = within(self.ask(http::Method::GET, &path, None, None)).await?;
        if answered.limited {
            return Err(limited_now());
        }
        if !answered.succeeded() {
            return Err(unavailable(format!("GitHub answered {} when asked for the app's bot", answered.status)));
        }
        let bot: Account = serde_json::from_str(&answered.body)
            .map_err(|e| unavailable(format!("what GitHub answered for the app's bot isn't a user: {e}")))?;
        Ok(bot.user())
    }
}

/// `text` as one part of an address's path, with anything but what GitHub's codes and slugs are made of escaped, so
/// none can reach another of GitHub's addresses.
pub(super) fn path_part(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
