use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE, LOCATION, USER_AGENT};
use http::{HeaderMap, Method, Request};
use http_body_util::{BodyExt as _, Collected, Full, LengthLimitError, Limited};
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use tokio::sync::OnceCell;
use url::Url;
use url::form_urlencoded::Serializer;

use super::reach::host;
use super::sending::{self, Tls};
use super::{Names, Reach};
use crate::{Failure, ServerUrl, ToolSignIn};

/// How long a call has, from looking up where to ask to the end of its answer, redirects included. A server answers in
/// well under a second, so this leaves room for a slow network, and doesn't leave a person waiting long on one that
/// isn't there.
const ANSWER_WITHIN: Duration = Duration::from_secs(15);

/// The most read of an answer. Metadata and tokens are a few KB.
pub(super) const LONGEST_ANSWER: usize = 256 * 1024;

/// The most redirects followed from one address, where a site that has moved needs one or two.
pub(super) const MOST_REDIRECTS: usize = 3;

const JSON: &str = "application/json";

/// The calls made for one server.
pub(crate) struct Caller<'a> {
    reach: Reach,
    names: &'a Names,
    tls: &'a OnceCell<Tls>,
}

/// What was answered, whatever it is.
pub(crate) struct Answer {
    pub(crate) host: String,
    pub(crate) status: u16,
    pub(crate) headers: HeaderMap,
    body: Bytes,
}

/// What's sent, beside where.
struct Sent<'a> {
    method: Method,
    headers: &'a [(&'a str, &'a str)],
    body: Bytes,
    reading: Reading,
}

/// How much of an answer is read.
enum Reading {
    Whole,
    /// Its status and headers, since its body may be a stream that doesn't end.
    Head,
}

impl ToolSignIn {
    /// Makes the calls for `server`.
    pub(crate) async fn calls_for(&self, server: &ServerUrl) -> Result<Caller<'_>, Failure> {
        let host = host(server.url());
        let reach = within(&host, async {
            let of = Reach::of(server, &self.names).await;
            of.map_err(|source| Failure::Unanswered { host: host.clone(), source: source.into() })
        });
        Ok(Caller { reach: reach.await?, names: &self.names, tls: &self.tls })
    }
}

impl Caller<'_> {
    /// `text` as an address that's fetched, or that a browser is sent to.
    pub(crate) fn address(&self, text: &str) -> Result<Url, Failure> {
        let what = "an address that isn't one";
        let url = Url::parse(text).map_err(|_| Failure::Unreadable { host: self.reach.host().to_string(), what })?;
        self.reach.takes(&url).map_err(|why| Failure::NotFetched { host: host(&url), why })?;
        Ok(url)
    }

    /// Asks for JSON at `url`, following redirects only to what it would have fetched at first.
    pub(crate) async fn get(&self, url: &Url) -> Result<Answer, Failure> {
        let headers = [(ACCEPT.as_str(), JSON)];
        let sent = Sent { method: Method::GET, headers: &headers, body: Bytes::new(), reading: Reading::Whole };
        let following = async {
            let mut url = url.clone();
            let mut answer = self.ask(&url, &sent).await?;
            for _ in 0..MOST_REDIRECTS {
                let Some(next) = moved(&url, &answer) else { break };
                url = next;
                answer = self.ask(&url, &sent).await?;
            }
            Ok(answer)
        };
        within(&host(url), following).await
    }

    /// Posts `form` to `url`. No redirect is followed, since it has secrets in it.
    pub(crate) async fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<Answer, Failure> {
        let body = Serializer::new(String::new()).extend_pairs(form).finish().into();
        let headers = [(ACCEPT.as_str(), JSON), (CONTENT_TYPE.as_str(), "application/x-www-form-urlencoded")];
        let sent = Sent { method: Method::POST, headers: &headers, body, reading: Reading::Whole };
        within(&host(url), self.ask(url, &sent)).await
    }

    /// Posts `body` to `url`. No redirect is followed, as RFC 7591 has none.
    pub(crate) async fn post_json(&self, url: &Url, body: &serde_json::Value) -> Result<Answer, Failure> {
        let headers = [(ACCEPT.as_str(), JSON), (CONTENT_TYPE.as_str(), JSON)];
        let sent =
            Sent { method: Method::POST, headers: &headers, body: body.to_string().into(), reading: Reading::Whole };
        within(&host(url), self.ask(url, &sent)).await
    }

    /// Posts an MCP message to `url`, with `headers`, and reads only the status and headers of the answer.
    pub(crate) async fn post_message(
        &self,
        url: &Url,
        headers: &[(&str, &str)],
        message: &serde_json::Value,
    ) -> Result<Answer, Failure> {
        let every = [(ACCEPT.as_str(), "application/json, text/event-stream"), (CONTENT_TYPE.as_str(), JSON)];
        let headers: Vec<_> = every.iter().chain(headers).copied().collect();
        let sent =
            Sent { method: Method::POST, headers: &headers, body: message.to_string().into(), reading: Reading::Head };
        within(&host(url), self.ask(url, &sent)).await
    }

    async fn ask(&self, url: &Url, sent: &Sent<'_>) -> Result<Answer, Failure> {
        let host = host(url);
        let refused = |why| Failure::NotFetched { host: host.clone(), why };
        let unanswered = |source| Failure::Unanswered { host: host.clone(), source };
        self.reach.takes(url).map_err(refused)?;
        let addresses = self.names.look_up(url).await.map_err(|e| unanswered(e.into()))?;
        let addresses = self.reach.vetted(url, addresses).map_err(refused)?;
        let mut request = Request::builder().method(&sent.method).uri(url.as_str()).header(USER_AGENT, "roundhouse");
        for (name, value) in sent.headers {
            request = request.header(*name, *value);
        }
        let request = request.body(Full::new(sent.body.clone())).map_err(|e| unanswered(e.into()))?;
        let response = sending::send(self.tls, addresses, request).await.map_err(unanswered)?;
        let (head, body) = response.into_parts();
        let body = match sent.reading {
            Reading::Whole => Limited::new(body, LONGEST_ANSWER).collect().await.map(Collected::to_bytes),
            Reading::Head => Ok(Bytes::new()),
        };
        let body = body.map_err(|source| {
            if source.is::<LengthLimitError>() {
                return Failure::Unreadable { host: host.clone(), what: "more than a sign-in has" };
            }
            unanswered(source)
        })?;
        Ok(Answer { host, status: head.status.as_u16(), headers: head.headers, body })
    }
}

#[cfg(test)]
impl<'a> Caller<'a> {
    pub(super) fn made_up(reach: Reach, names: &'a Names, tls: &'a OnceCell<Tls>) -> Self {
        Self { reach, names, tls }
    }
}

impl Answer {
    /// Whether it did as it was asked.
    pub(crate) fn succeeded(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Whether it was refused, for a reason that may be in its body, rather than only for now.
    pub(crate) fn refused(&self) -> bool {
        (400..500).contains(&self.status) && self.status != 429
    }

    /// What its status comes to, where that's all it says.
    pub(crate) fn failure(self) -> Failure {
        let Answer { host, status, .. } = self;
        match status {
            429 | 500.. => Failure::Busy { host, status },
            _ => Failure::Answered { host, status },
        }
    }
}

/// What `answer`'s body says, if it's JSON that says it.
pub(crate) fn json<T: DeserializeOwned>(answer: &Answer) -> Option<T> {
    serde_json::from_slice(&answer.body).ok()
}

/// Where `answer`, which `url` gave, says to ask instead.
fn moved(url: &Url, answer: &Answer) -> Option<Url> {
    if !matches!(answer.status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    url.join(answer.headers.get(LOCATION)?.to_str().ok()?).ok()
}

/// What `call`, of `host`, comes to within [`ANSWER_WITHIN`], whatever stage it hangs at.
async fn within<T>(host: &str, call: impl Future<Output = Result<T, Failure>>) -> Result<T, Failure> {
    let Ok(answer) = tokio::time::timeout(ANSWER_WITHIN, call).await else {
        return Err(Failure::TimedOut { host: host.to_string() });
    };
    answer
}
