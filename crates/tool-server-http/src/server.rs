use http::header::{HeaderMap, HeaderName, HeaderValue};
use http::{Method, Request, Response, Uri};
use http_body_util::Full;
use hyper::body::Bytes;
use protocol::model::ToolServerUrl;
use protocol::worker::ServerHeader;

use crate::redirect::{MOST_REDIRECTS_WITHIN, Redirect, redirect};

/// A server, at the URL the person typed, and with the header that says who's asking, which goes with every request to
/// it, so an agent's call and the hub's check carry the same.
pub struct Server {
    uri: Uri,
    header: Option<(HeaderName, HeaderValue)>,
}

/// Why a server's answer isn't one: it sent the request out of its origin, as another request, or around more times
/// than a server that has moved needs, so it wasn't followed.
#[derive(Debug, PartialEq, Eq)]
pub struct Redirected;

impl Server {
    /// `None` for a URL that isn't HTTP's, or a header HTTP can't carry, which the hub lets nobody give.
    pub fn new(url: &ToolServerUrl, header: Option<&ServerHeader>) -> Option<Self> {
        let uri: Uri = url.as_str().parse().ok()?;
        uri.host()?;
        if !matches!(uri.scheme_str(), Some("http" | "https")) {
            return None;
        }
        let header = match header {
            Some(header) => Some(sent(header)?),
            None => None,
        };
        Some(Self { uri, header })
    }

    /// Whether it's sent a key, or another header that says who's asking.
    pub fn keyed(&self) -> bool {
        self.header.is_some()
    }

    /// Its answer to `method`, with `headers` and `body`, which `sending` sends. A redirect within its origin that keeps
    /// the method and the body is followed, so the request, and the header, only ever go to the origin they were given
    /// for, whatever sends them.
    pub async fn send<B, E, F>(
        &self,
        method: &Method,
        headers: &HeaderMap,
        body: &Bytes,
        mut sending: impl FnMut(Request<Full<Bytes>>) -> F,
    ) -> Result<Result<Response<B>, Redirected>, E>
    where
        F: Future<Output = Result<Response<B>, E>>,
    {
        let mut to = self.uri.clone();
        for _ in 0..=MOST_REDIRECTS_WITHIN {
            let response = sending(self.request(&to, method, headers, body)).await?;
            match redirect(&to, response.status(), response.headers()) {
                Redirect::Not => return Ok(Ok(response)),
                Redirect::Within(within) => to = within,
                Redirect::Elsewhere => break,
            }
        }
        Ok(Err(Redirected))
    }

    /// What's sent `to`, with the server's own header over any of its name in `headers`.
    fn request(&self, to: &Uri, method: &Method, headers: &HeaderMap, body: &Bytes) -> Request<Full<Bytes>> {
        let mut request = Request::new(Full::new(body.clone()));
        *request.method_mut() = method.clone();
        *request.uri_mut() = to.clone();
        *request.headers_mut() = headers.clone();
        if let Some((name, value)) = &self.header {
            request.headers_mut().insert(name, value.clone());
        }
        request
    }
}

/// `header` as it's sent, which a log of the request leaves out.
fn sent(header: &ServerHeader) -> Option<(HeaderName, HeaderValue)> {
    let name = HeaderName::from_bytes(header.name.as_str().as_bytes()).ok()?;
    let mut value = HeaderValue::from_str(header.value.as_str()).ok()?;
    value.set_sensitive(true);
    Some((name, value))
}
