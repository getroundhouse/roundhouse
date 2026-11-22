//! Requests to App Store Connect's API, each with a fresh token.

use std::time::Duration;

use anyhow::{Context as _, bail};
use serde_json::Value;

use super::Key;

const API: &str = "https://api.appstoreconnect.apple.com";
/// For the whole of one request, which App Store Connect answers within seconds.
const REQUEST_WITHIN: Duration = Duration::from_secs(60);

pub(crate) struct Client<'a> {
    agent: ureq::Agent,
    key: &'a Key,
}

impl<'a> Client<'a> {
    pub(crate) fn new(key: &'a Key) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(REQUEST_WITHIN)).https_only(true);
        Self { agent: config.http_status_as_error(false).build().into(), key }
    }

    /// The JSON App Store Connect answers `path`, such as `/v1/apps?filter[bundleId]=…`, with.
    pub(crate) fn get(&self, path: &str) -> anyhow::Result<Value> {
        let request = self.agent.get(format!("{API}{path}")).header("Authorization", self.authorization()?);
        answer("GET", path, request.call())
    }

    /// Sends `body` to `path` with `method`, `POST` or `PATCH`, and returns the JSON App Store Connect answers with.
    pub(crate) fn send(&self, method: Method, path: &str, body: &Value) -> anyhow::Result<Value> {
        let url = format!("{API}{path}");
        let request = match method {
            Method::Post => self.agent.post(url),
            Method::Patch => self.agent.patch(url),
        };
        let request = request.header("Authorization", self.authorization()?).header("Content-Type", "application/json");
        answer(method.name(), path, request.send(body.to_string()))
    }

    fn authorization(&self) -> anyhow::Result<String> {
        Ok(format!("Bearer {}", self.key.token(jiff::Timestamp::now().as_second())?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Post,
    Patch,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Patch => "PATCH",
        }
    }
}

/// The body of a response that succeeded, as JSON, or what App Store Connect said was wrong.
fn answer(
    method: &str,
    path: &str,
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> anyhow::Result<Value> {
    let mut response = response.with_context(|| format!("couldn't reach App Store Connect for {method} {path}"))?;
    let status = response.status().as_u16();
    let body =
        response.body_mut().read_to_string().with_context(|| format!("couldn't read {method} {path}'s answer"))?;
    if !(200..300).contains(&status) {
        bail!("App Store Connect refused {method} {path} with {status}: {body}");
    }
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).with_context(|| format!("App Store Connect's answer to {method} {path} isn't JSON"))
}
