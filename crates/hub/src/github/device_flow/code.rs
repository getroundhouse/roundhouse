use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::GithubUserCode;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::super::Github;
use super::super::client::{unavailable, within};
use super::refusal;

/// What a sign-in asks GitHub for: what an agent could need on GitHub, as it could with the person's own login on
/// their machine. Without `workflow`, GitHub refuses a push that changes `.github/workflows/`. Left out are
/// `delete_repo` and the `admin:` scopes, which `gh` asks for only when told to. Narrower access is a fine-grained
/// token, connected as one. Space between, as GitHub's docs give them.
pub(crate) const SCOPES: &str = "repo workflow read:org gist project write:packages user:email";

/// How long to wait between two asks after a sign-in's token when GitHub says nothing of it, as its docs give.
pub(super) const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);

protocol::tokens!(
    /// What the hub asks GitHub with how a sign-in stands, and takes its token with once it's approved. With the app's
    /// client ID, which is no secret, it's all it takes, so the hub keeps it only in memory, and only while the
    /// sign-in waits.
    GithubDeviceCode,
);

/// A sign-in's code, as GitHub gave it.
#[derive(Debug)]
pub(crate) struct DeviceCode {
    pub(crate) device: GithubDeviceCode,
    /// What the person types to approve it.
    pub(crate) user_code: GithubUserCode,
    /// Where they type it, such as `https://github.com/login/device`.
    pub(crate) address: String,
    /// How long it lasts from when GitHub gave it.
    pub(crate) lasts: Duration,
    /// When it ends, by tokio's clock, from when GitHub's answer came.
    pub(super) ends: Instant,
    /// How long to wait between two asks after its token, as GitHub first said, or [`DEFAULT_INTERVAL`] when it said
    /// nothing of it. Asking keeps to the hub's least all the same, as [`paced`](super::pacing::paced) says.
    pub(super) interval: Duration,
}

/// What `POST /login/device/code` is sent.
#[derive(Serialize)]
struct Asking<'a> {
    client_id: &'a str,
    scope: &'a str,
}

/// What `POST /login/device/code` answers with, as far as the hub reads it.
#[derive(Deserialize)]
struct Given {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

impl Github {
    /// A new code for a sign-in, through the OAuth App, for [`SCOPES`], as `POST /login/device/code` gives one, within
    /// [`ASK_WITHIN`](super::super::ASK_WITHIN). Refused with `Unavailable` when GitHub can't be reached, doesn't
    /// answer in time, or answers with anything but a code, as when it's limiting how often it's asked, or the app
    /// doesn't let people sign in through the device flow.
    pub(crate) async fn device_code(&self) -> Result<DeviceCode, ApiError> {
        let asking = Asking { client_id: self.config.client_id.as_str(), scope: SCOPES };
        let (status, body) = within(self.post_to_login("/login/device/code", &asking)).await?;
        let given = Instant::now();
        if !(200..300).contains(&status) {
            return Err(unavailable(format!("GitHub answered {status} when asked for a sign-in's code")));
        }
        let code: Given = serde_json::from_str(&body).map_err(|_| refusal("for a sign-in's code", &body))?;
        let lasts = Duration::from_secs(code.expires_in);
        let interval = code.interval.map_or(DEFAULT_INTERVAL, Duration::from_secs);
        Ok(DeviceCode {
            device: GithubDeviceCode::new(code.device_code),
            user_code: code.user_code.into(),
            address: code.verification_uri,
            lasts,
            ends: given + lasts,
            interval,
        })
    }
}
