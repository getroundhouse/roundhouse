//! The builds of the worker for machines other than the hub's, which the hub gives a provider's adapter to put on an
//! instance as it starts, so a worker there runs the hub's build.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use protocol::model::WorkerTarget;
use providers::WorkerBinary;
use serde::Deserialize;
use tracing::error;

/// A worker build, as the providers file names it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerEntry {
    target: WorkerTarget,
    path: PathBuf,
}

/// Each of `entries` with its file's SHA-256, by which an adapter tells whether an instance holds it already. One whose
/// file can't be read is left out, and says why, so an adapter that needs it says the hub has no worker for its
/// target, rather than the hub giving it a path that isn't there.
pub(super) fn with_checksums(entries: Vec<WorkerEntry>) -> Vec<WorkerBinary> {
    let mut builds = Vec::new();
    for WorkerEntry { target, path } in entries {
        match sha256(&path) {
            Ok(sha256) => {
                builds.push(WorkerBinary { target, path: path.to_string_lossy().into_owned(), sha256 });
            }
            Err(e) => error!(%target, path = %path.display(), "leaving out a worker build that can't be read: {e}"),
        }
    }
    builds
}

/// The SHA-256 of the file at `path`, in lowercase hex.
fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hash.finish().as_ref() {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}
