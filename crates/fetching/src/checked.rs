use std::fs::{File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use ring::digest::{Context, SHA256};

use super::fetcher::{answer, read_at_most};
use super::{FetchError, Fetcher, FolderLock};

/// What a file should be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expected {
    pub size_bytes: u64,
    pub sha256: [u8; 32],
}

/// How a checked fetch ended, when it didn't fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The file is in place, and is the one expected.
    Done,
    /// Its `progress` said to stop. What came is kept for next time.
    Stopped,
}

/// Why a checked fetch failed.
#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("{url} isn't the file it should be")]
    NotTheFile { url: String },
}

/// How often hashing what's already here says how far it has got, so a stop is heard meanwhile.
const HASHING_BY_BYTES: usize = 1 << 20;

impl Fetcher {
    /// The file `expected` describes, from `url` to `to` in `lock`'s folder, from a server that answers ranges. It
    /// goes on from what's in `to`'s `.part`, which it hashes first, and asks for the rest with a range, or starts again
    /// if the server sends the file whole. It hashes the rest as it comes, and puts the file in place only once its
    /// hash is right, throwing the `.part` away if it isn't. Only one fetch writes the `.part` at a time, since it has
    /// to hold the lock, so what it hashes is what it wrote.
    ///
    /// `progress` is told how many bytes of the file are here, as the `.part` is hashed and at each read, and stops the
    /// fetch by giving `Break`. What's come is kept for next time, as it is when the fetch fails partway. A file is
    /// synced to disk before it's put in place, so one there after a crash is whole.
    pub fn checked_file(
        &self,
        lock: &FolderLock,
        url: &str,
        to: &Path,
        expected: &Expected,
        progress: &mut dyn FnMut(u64) -> ControlFlow<()>,
    ) -> Result<Fetched, CheckError> {
        assert!(to.is_relative(), "{} has to be under the locked folder", to.display());
        let to = lock.dir().join(to);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(saving(parent))?;
        }
        let part = PathBuf::from(format!("{}.part", to.display()));
        let mut hash = Context::new(&SHA256);
        let Some(mut here) = hashed(&part, &mut hash, progress).map_err(saving(&part))? else {
            return Ok(Fetched::Stopped);
        };
        if here > expected.size_bytes {
            (here, hash) = (0, Context::new(&SHA256));
        }
        if here < expected.size_bytes {
            match self.rest(url, &part, expected.size_bytes, (here, hash), progress) {
                Ok(Some(whole)) => hash = whole,
                Ok(None) => return Ok(Fetched::Stopped),
                // More came than the file's length.
                Err(FetchError::TooLong { .. }) => return not_the_file(&part, url),
                Err(e) => return Err(e.into()),
            }
        }
        if hash.finish().as_ref() != expected.sha256 {
            return not_the_file(&part, url);
        }
        // Whichever way it became whole, here or in a fetch that stopped or ended before syncing it.
        OpenOptions::new().write(true).open(&part).and_then(|whole| whole.sync_all()).map_err(saving(&part))?;
        std::fs::rename(&part, &to).map_err(saving(&to))?;
        let _ = progress(expected.size_bytes);
        Ok(Fetched::Done)
    }

    /// Fetches what isn't in `part` yet of a file `size_bytes` long, as `got` says how much is and its hash so far, and
    /// gives the hash of all of it, or `None` once `progress` says to stop.
    fn rest(
        &self,
        url: &str,
        part: &Path,
        size_bytes: u64,
        got: (u64, Context),
        progress: &mut dyn FnMut(u64) -> ControlFlow<()>,
    ) -> Result<Option<Context>, FetchError> {
        let (mut here, mut hash) = got;
        // As it's stored, since a range of a compressed answer isn't a range of the file.
        let mut request = self.get(url).header("Accept-Encoding", "identity");
        if here > 0 {
            request = request.header("Range", format!("bytes={here}-"));
        }
        let response = answer(url, request, |status| status == 200 || status == 206 && here > 0)?;
        if response.status() == 200 {
            // The whole file, from the start.
            (here, hash) = (0, Context::new(&SHA256));
        }
        let open = OpenOptions::new().create(true).write(true).append(here > 0).truncate(here == 0).open(part);
        let mut out = open.map_err(saving(part))?;
        if progress(here).is_break() {
            return Ok(None);
        }
        let read = read_at_most(url, response, size_bytes - here, |chunk| {
            out.write_all(chunk).map_err(saving(part))?;
            hash.update(chunk);
            here += chunk.len() as u64;
            Ok(progress(here))
        })?;
        if read.is_break() {
            return Ok(None);
        }
        if here < size_bytes {
            return Err(FetchError::CutShort { url: url.to_string(), expected_bytes: size_bytes, got_bytes: here });
        }
        Ok(Some(hash))
    }
}

/// Throws away `part`, which isn't the start of the file at `url`, so the next fetch starts it again.
fn not_the_file(part: &Path, url: &str) -> Result<Fetched, CheckError> {
    std::fs::remove_file(part).map_err(saving(part))?;
    Err(CheckError::NotTheFile { url: url.to_string() })
}

/// What failing to write to `path` is.
fn saving(path: &Path) -> impl FnOnce(io::Error) -> FetchError + '_ {
    move |source| FetchError::Save { path: path.to_path_buf(), source }
}

/// Puts what's in `path` through `hash`, telling `progress` how much has been, and gives its length: nothing, if it
/// isn't there, and `None` once `progress` says to stop.
fn hashed(
    path: &Path,
    hash: &mut Context,
    progress: &mut dyn FnMut(u64) -> ControlFlow<()>,
) -> io::Result<Option<u64>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Some(0)),
        Err(e) => return Err(e),
    };
    let (mut buffer, mut length) = (vec![0; HASHING_BY_BYTES], 0);
    loop {
        if progress(length).is_break() {
            return Ok(None);
        }
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(Some(length));
        }
        hash.update(&buffer[..read]);
        length += read as u64;
    }
}
