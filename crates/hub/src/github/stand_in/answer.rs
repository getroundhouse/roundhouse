use std::sync::Arc;

use serde_json::json;
use tokio::sync::watch;

use super::MADE_UP_APP_KEY;

/// What the stand-in answers a call with.
#[derive(Clone)]
pub enum Answer {
    /// This status, these headers and this body.
    With { status: u16, headers: Vec<(&'static str, &'static str)>, body: String },
    /// Nothing at all: it takes the request and holds the connection open.
    Never,
    /// What the boxed answer says, once the gate has been opened.
    After(Gate, Box<Answer>),
}

/// What holds an answer back until a test opens it, as [`Answer::After`] says.
#[derive(Clone)]
pub struct Gate(Arc<watch::Sender<bool>>);

impl Gate {
    /// One no answer has passed yet.
    pub fn closed() -> Self {
        Gate(Arc::new(watch::Sender::new(false)))
    }

    /// Lets every answer it holds back go, and every one after.
    pub fn open(&self) {
        self.0.send_replace(true);
    }

    pub(super) async fn opened(&self) {
        self.0.subscribe().wait_for(|open| *open).await.unwrap();
    }
}

impl Answer {
    /// The user `login`, whose number is `id`, as `GET /user` gives it, with the header that says when its token ends,
    /// as GitHub writes it, such as `2027-02-11 08:00:00 UTC`.
    pub fn user(id: u64, login: &str, ends: Option<&'static str>) -> Self {
        let headers = ends.map(|ends| ("github-authentication-token-expiration", ends)).into_iter().collect();
        let body = json!({"login": login, "id": id, "type": "User"}).to_string();
        Answer::With { status: 200, headers, body }
    }

    /// Only `status`, with a body that says nothing more, as GitHub's errors are.
    pub fn status(status: u16, headers: Vec<(&'static str, &'static str)>) -> Self {
        Answer::With { status, headers, body: r#"{"message":"made up"}"#.to_string() }
    }

    /// A sign-in's code, `device` to ask with, for the person to type as `WDJB-MJHT`, lasting `expires_in` seconds with
    /// `interval` seconds between asks, as `POST /login/device/code` gives one.
    pub fn code(device: &str, expires_in: u64, interval: u64) -> Self {
        let body = json!({
            "device_code": device,
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://github.com/login/device",
            "expires_in": expires_in,
            "interval": interval,
        });
        Self::json(&body)
    }

    /// The token `token`, which lasts 8 hours, and `refresh`, which lasts about 181 days, as GitHub grants a sign-in.
    pub fn granted(token: &str, refresh: &str) -> Self {
        let body = json!({
            "access_token": token,
            "token_type": "bearer",
            "scope": "gist,project,read:org,repo,user:email,workflow,write:packages",
            "expires_in": 28_800,
            "refresh_token": refresh,
            "refresh_token_expires_in": 15_638_400,
        });
        Self::json(&body)
    }

    /// The error `error` of GitHub's device flow, such as `authorization_pending`, which it answers with a 200.
    pub fn device_flow_error(error: &str) -> Self {
        let body = json!({"error": error, "error_description": "Made up.", "error_uri": "https://docs"});
        Self::json(&body)
    }

    /// GitHub saying it's asked too often, with the interval to keep to from then on, if it says one.
    pub fn slow_down(interval: Option<u64>) -> Self {
        let mut body = json!({"error": "slow_down", "error_description": "Too many requests."});
        if let Some(interval) = interval {
            body["interval"] = interval.into();
        }
        Self::json(&body)
    }

    /// The app `id`, `slug`, under the organization `owner`, by number and name, with [`MADE_UP_APP_KEY`] as its key,
    /// as `POST /app-manifests/{code}/conversions` makes one, with the secrets beside it that the hub doesn't keep.
    pub fn app_made(id: u64, slug: &str, owner: (u64, &str)) -> Self {
        let (owner_id, owner) = owner;
        let body = json!({
            "id": id,
            "slug": slug,
            "name": format!("{slug}, made up"),
            "owner": {"login": owner, "id": owner_id, "type": "Organization"},
            "client_id": "Iv23made-up",
            "client_secret": "a-made-up-client-secret",
            "webhook_secret": null,
            "pem": MADE_UP_APP_KEY,
        });
        Answer::With { status: 201, headers: Vec::new(), body: body.to_string() }
    }

    /// An app's installations, each its number and the organization it's on, by number and name, as
    /// `GET /app/installations` lists them.
    pub fn installations(installations: &[(u64, u64, &str)]) -> Self {
        let listed: Vec<_> = installations
            .iter()
            .map(|(id, on, name)| json!({"id": id, "account": {"login": name, "id": on, "type": "Organization"}}))
            .collect();
        Self::json(&json!(listed))
    }

    /// An installation's token, `token`, ending at `ends`, as GitHub writes it, such as `2026-11-16T10:00:00Z`, as
    /// `POST /app/installations/{id}/access_tokens` gives one.
    pub fn installation_token(token: &str, ends: &str) -> Self {
        let body = json!({"token": token, "expires_at": ends, "permissions": {"contents": "write"}});
        Answer::With { status: 201, headers: Vec::new(), body: body.to_string() }
    }

    fn json(body: &serde_json::Value) -> Self {
        Answer::With { status: 200, headers: Vec::new(), body: body.to_string() }
    }
}
