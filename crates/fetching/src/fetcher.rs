use std::convert::Infallible;
use std::fs::File;
use std::io;
use std::io::{Read, Write as _};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Why a fetch failed. Each says what it was fetching, or saving to.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// It couldn't be asked, or didn't answer in time.
    #[error("couldn't fetch {url}: {source}")]
    Request { url: String, source: ureq::Error },
    /// It answered with something other than the file, such as 404, or a redirect that isn't followed.
    #[error("{url} answered {status} rather than the file")]
    Status { url: String, status: u16 },
    #[error("couldn't read {url}: {source}")]
    Read { url: String, source: io::Error },
    #[error("{url} ended after {got_bytes} of its {expected_bytes} bytes")]
    CutShort { url: String, expected_bytes: u64, got_bytes: u64 },
    #[error("{url} is longer than the {limit_bytes} bytes it should be")]
    TooLong { url: String, limit_bytes: u64 },
    #[error("couldn't save {path}: {source}")]
    Save { path: PathBuf, source: io::Error },
}

impl FetchError {
    /// Whether it failed because the server didn't answer, or stopped sending, within the time it had.
    pub fn timed_out(&self) -> bool {
        match self {
            Self::Request { source, .. } => matches!(source, ureq::Error::Timeout(_)),
            // A body's reader gives ureq's own error inside its own.
            Self::Read { source, .. } => {
                let inner = source.get_ref().and_then(|inner| inner.downcast_ref::<ureq::Error>());
                matches!(inner, Some(ureq::Error::Timeout(_)))
            }
            Self::Status { .. } | Self::CutShort { .. } | Self::TooLong { .. } | Self::Save { .. } => false,
        }
    }
}

/// Fetches files over HTTP(S). A fetch that starts on HTTPS stays on it, wherever a redirect sends it.
pub struct Fetcher {
    agent: ureq::Agent,
}

impl Fetcher {
    /// For a small file that should come at once, from exactly where it's asked for: each request has `timeout` for
    /// the whole of it, and a redirect isn't followed, so it's an answer like any other.
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(timeout)).max_redirects(0);
        Self { agent: config.http_status_as_error(false).build().into() }
    }

    /// For a big file, from a host that may send it from elsewhere, as Hugging Face does from its storage: a request
    /// goes on for as long as each read comes within `stalled`, and follows redirects.
    pub fn for_big_files(stalled: Duration) -> Self {
        let config = ureq::Agent::config_builder().timeout_connect(Some(stalled)).timeout_per_read(Some(stalled));
        Self { agent: config.http_status_as_error(false).build().into() }
    }

    /// A request for `url`, which stays on HTTPS if it starts there.
    pub(crate) fn get(&self, url: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        self.agent.get(url).config().https_only(url.starts_with("https://")).build()
    }

    /// `url`'s body, once decompressed, refused if it's longer than `limit_bytes`.
    pub fn bytes(&self, url: &str, limit_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let mut body = Vec::new();
        let ControlFlow::Continue(_) = read_at_most(url, self.call(url)?, limit_bytes, |chunk| {
            body.extend_from_slice(chunk);
            Ok(ControlFlow::<Infallible>::Continue(()))
        })?;
        Ok(body)
    }

    /// `url`'s body, once decompressed, written to `to`, which has to be exactly `size_bytes` long. What's written of a
    /// body that isn't is left for the caller to remove.
    pub fn to_file(&self, url: &str, to: &Path, size_bytes: u64) -> Result<(), FetchError> {
        let response = self.call(url)?;
        let saving = |source| FetchError::Save { path: to.to_path_buf(), source };
        let mut file = File::create(to).map_err(saving)?;
        let ControlFlow::Continue(got_bytes) = read_at_most(url, response, size_bytes, |chunk| {
            file.write_all(chunk).map_err(saving)?;
            Ok(ControlFlow::<Infallible>::Continue(()))
        })?;
        if got_bytes < size_bytes {
            return Err(FetchError::CutShort { url: url.to_string(), expected_bytes: size_bytes, got_bytes });
        }
        Ok(())
    }

    /// The response, if it's the file.
    fn call(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, FetchError> {
        answer(url, self.get(url), |status| status == 200)
    }
}

/// What `request`, for `url`, is answered with, if its status is one `takes`.
pub(crate) fn answer(
    url: &str,
    request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    takes: impl FnOnce(u16) -> bool,
) -> Result<ureq::http::Response<ureq::Body>, FetchError> {
    let response = request.call().map_err(|source| FetchError::Request { url: url.to_string(), source })?;
    match response.status().as_u16() {
        status if takes(status) => Ok(response),
        status => Err(FetchError::Status { url: url.to_string(), status }),
    }
}

/// Hands the body to `write` as it arrives, once decompressed, and gives its length, unless `write` says to stop. It
/// reads one byte past `limit_bytes` at most, so a longer body is refused, rather than cut to fit or read whole, and
/// `write` is given none of what's past it.
pub(crate) fn read_at_most<B>(
    url: &str,
    response: ureq::http::Response<ureq::Body>,
    limit_bytes: u64,
    mut write: impl FnMut(&[u8]) -> Result<ControlFlow<B>, FetchError>,
) -> Result<ControlFlow<B, u64>, FetchError> {
    let mut body = response.into_body().into_reader().take(limit_bytes.saturating_add(1));
    let (mut buffer, mut got_bytes) = (vec![0; 1 << 16], 0);
    loop {
        let read = body.read(&mut buffer).map_err(|source| FetchError::Read { url: url.to_string(), source })?;
        if read == 0 {
            return Ok(ControlFlow::Continue(got_bytes));
        }
        got_bytes += read as u64;
        if got_bytes > limit_bytes {
            return Err(FetchError::TooLong { url: url.to_string(), limit_bytes });
        }
        if let ControlFlow::Break(stop) = write(&buffer[..read])? {
            return Ok(ControlFlow::Break(stop));
        }
    }
}
