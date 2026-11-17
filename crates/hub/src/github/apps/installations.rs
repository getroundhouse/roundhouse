use std::time::Duration;

use chrono::DateTime;
use protocol::client::ApiError;
use protocol::model::{GithubAppId, GithubInstallationId, GithubToken, GithubUser};
use serde::Deserialize;
use tracing::warn;

use super::super::Github;
use super::super::client::{Answered, limited_now, unavailable, within};
use super::GithubAppKey;
use super::made::Account;
use crate::store::{later_ms, now_ms};

/// How long an installation's token lasts, as GitHub's docs say every one does, which the hub takes when it can't read
/// when GitHub says one ends.
const INSTALLATION_TOKEN_LASTS: Duration = Duration::from_hours(1);

/// How many installations GitHub lists a page, at most, which is the most it gives.
const PER_PAGE: usize = 100;

/// How many pages of installations the hub reads at most, which is far more than one person installs an app on, so it
/// stops at a GitHub that never stops listing.
const PAGES: usize = 10;

/// One installation of an app, as GitHub lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Installation {
    pub(crate) id: GithubInstallationId,
    /// The user or organization it's on.
    pub(crate) on: GithubUser,
}

/// What minting an installation's token came to.
#[derive(Debug)]
pub(crate) enum Minted {
    /// A token that works as the app's bot where it's installed, until `ends_at_ms`, by the hub's clock.
    Token { token: GithubToken, ends_at_ms: i64 },
    /// GitHub has no such installation, as once it's been uninstalled.
    Gone,
}

/// An installation, as `GET /app/installations` lists one, as far as the hub reads it. One on an enterprise names no
/// user or organization, and isn't one an agent pushes through.
#[derive(Deserialize)]
struct Listed {
    id: u64,
    #[serde(default)]
    account: Option<serde_json::Value>,
}

/// What `POST /app/installations/{id}/access_tokens` answers that the hub reads.
#[derive(Deserialize)]
struct Given {
    token: String,
    #[serde(default)]
    expires_at: Option<String>,
}

impl Github {
    /// Every installation of `app`, as GitHub lists them to the app, asked with a JWT signed with `key`, a page at a
    /// time, each within [`ASK_WITHIN`](super::super::ASK_WITHIN). Refused with `Conflict` for a key the hub can't
    /// sign with, and with `Unavailable` when GitHub can't be reached, doesn't answer in time, refuses the JWT, as
    /// when the app has been deleted or its key revoked, or lists more pages than [`PAGES`], so a list it answers with
    /// is every installation there is.
    pub(crate) async fn installations(
        &self,
        app: GithubAppId,
        key: &GithubAppKey,
    ) -> Result<Vec<Installation>, ApiError> {
        let mut installations = Vec::new();
        for page in 1..=PAGES {
            let jwt = key.signed_as(app, now_ms())?;
            let path = format!("/app/installations?per_page={PER_PAGE}&page={page}");
            let answered = within(self.ask(http::Method::GET, &path, Some(jwt.as_str()), None)).await?;
            let listed: Vec<Listed> = serde_json::from_str(&as_app(&answered, "for its installations")?.body)
                .map_err(|e| unavailable(format!("what GitHub listed of the app's installations isn't a list: {e}")))?;
            let last = listed.len() < PER_PAGE;
            installations.extend(listed.into_iter().filter_map(|listed| {
                let on = serde_json::from_value::<Account>(listed.account?).ok()?;
                Some(Installation { id: GithubInstallationId::new(listed.id), on: on.user() })
            }));
            if last {
                return Ok(installations);
            }
        }
        Err(unavailable(format!("GitHub listed more than {PAGES} pages of the app's installations")))
    }

    /// A new token for `installation` of `app`, asked with a JWT signed with `key`, within
    /// [`ASK_WITHIN`](super::super::ASK_WITHIN), which GitHub has last an hour. One minted takes nothing from the
    /// last, which works on until it ends. Refused as [`Github::installations`] is.
    pub(crate) async fn installation_token(
        &self,
        app: GithubAppId,
        key: &GithubAppKey,
        installation: GithubInstallationId,
    ) -> Result<Minted, ApiError> {
        let jwt = key.signed_as(app, now_ms())?;
        let path = format!("/app/installations/{installation}/access_tokens");
        let answered = within(self.ask(http::Method::POST, &path, Some(jwt.as_str()), None)).await?;
        let given_at_ms = now_ms();
        if answered.status == 404 {
            return Ok(Minted::Gone);
        }
        let given: Given = serde_json::from_str(&as_app(&answered, "for an installation's token")?.body)
            .map_err(|_| unavailable("what GitHub answered when asked for an installation's token isn't one"))?;
        let ends = given.expires_at.as_deref().and_then(|said| DateTime::parse_from_rfc3339(said).ok());
        let ends_at_ms = ends.map_or_else(
            || {
                warn!(said = ?given.expires_at, "couldn't read when GitHub says an installation's token ends");
                later_ms(given_at_ms, INSTALLATION_TOKEN_LASTS)
            },
            |ends| ends.timestamp_millis(),
        );
        Ok(Minted::Token { token: GithubToken::new(given.token), ends_at_ms })
    }
}

/// `answered`, when GitHub did as the app asked it `what`, and otherwise `Unavailable`, saying why.
fn as_app<'a>(answered: &'a Answered, what: &str) -> Result<&'a Answered, ApiError> {
    if answered.limited {
        return Err(limited_now());
    }
    match answered.status {
        _ if answered.succeeded() => Ok(answered),
        401 => Err(unavailable(format!(
            "GitHub refused the app's key when asked {what}, as when the app has been deleted or its key revoked"
        ))),
        status => Err(unavailable(format!("GitHub answered {status} when asked {what}"))),
    }
}
