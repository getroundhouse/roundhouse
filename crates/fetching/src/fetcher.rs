use std::fs::File;
use std::io;
use std::io::{Read, Write as _};
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

/// Fetches files over HTTP(S).
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

    /// `url`'s body, once decompressed, refused if it's longer than `limit_bytes`.
    pub fn bytes(&self, url: &str, limit_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let mut body = Vec::new();
        read_at_most(url, self.call(url)?, limit_bytes, |chunk| {
            body.extend_from_slice(chunk);
            Ok(())
        })?;
        Ok(body)
    }

    /// `url`'s body, once decompressed, written to `to`, which has to be exactly `size_bytes` long. What's written of a
    /// body that isn't is left for the caller to remove.
    pub fn to_file(&self, url: &str, to: &Path, size_bytes: u64) -> Result<(), FetchError> {
        let response = self.call(url)?;
        let saving = |source| FetchError::Save { path: to.to_path_buf(), source };
        let mut file = File::create(to).map_err(saving)?;
        let got_bytes = read_at_most(url, response, size_bytes, |chunk| file.write_all(chunk).map_err(saving))?;
        if got_bytes < size_bytes {
            return Err(FetchError::CutShort { url: url.to_string(), expected_bytes: size_bytes, got_bytes });
        }
        Ok(())
    }

    /// The response, if it's the file.
    fn call(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, FetchError> {
        let response =
            self.agent.get(url).call().map_err(|source| FetchError::Request { url: url.to_string(), source })?;
        match response.status().as_u16() {
            200 => Ok(response),
            status => Err(FetchError::Status { url: url.to_string(), status }),
        }
    }
}

/// Hands the body to `write` as it arrives, once decompressed, and returns its length. It reads one byte past
/// `limit_bytes` at most, so a longer body is refused, rather than cut to fit or read whole.
fn read_at_most(
    url: &str,
    response: ureq::http::Response<ureq::Body>,
    limit_bytes: u64,
    mut write: impl FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<u64, FetchError> {
    let mut body = response.into_body().into_reader().take(limit_bytes.saturating_add(1));
    let (mut buffer, mut got_bytes) = (vec![0; 1 << 16], 0);
    loop {
        let read = body.read(&mut buffer).map_err(|source| FetchError::Read { url: url.to_string(), source })?;
        if read == 0 {
            break;
        }
        write(&buffer[..read])?;
        got_bytes += read as u64;
    }
    if got_bytes > limit_bytes {
        return Err(FetchError::TooLong { url: url.to_string(), limit_bytes });
    }
    Ok(got_bytes)
}
