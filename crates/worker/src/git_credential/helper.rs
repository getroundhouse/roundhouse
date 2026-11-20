//! The worker's credential helper, which git runs for github.com in every process the worker gives its git config,
//! as a hidden command of the worker's own binary, and not `gh auth git-credential`, so pushing doesn't need `gh`
//! installed.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use protocol::worker::GithubCredential;

use super::files::ANSWER;

/// Said when what the helper answers with can't be read, which only a worker that's stopping, or a file from a newer
/// worker, leaves.
const UNREADABLE: &str =
    "Roundhouse couldn't read which GitHub account agents on this machine push as, so they push as nobody.";

/// The program a worker's git config names as git's credential helper for github.com, and its arguments, after which
/// it's given the worker's directory of files, and git the action it asks for. The worker's own binary, whose hidden
/// command runs [`answer_credential_request`].
#[derive(Clone, Debug)]
pub struct CredentialHelper {
    pub(super) program: PathBuf,
    pub(super) args: Vec<OsString>,
}

impl CredentialHelper {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        Self { program: program.into(), args: args.into_iter().map(Into::into).collect() }
    }

    pub(super) fn words(&self) -> Vec<OsString> {
        std::iter::once(self.program.clone().into_os_string()).chain(self.args.iter().cloned()).collect()
    }
}

/// Answers git's `action` as its credential helper, for the worker whose files are in `dir`, reading what git asks
/// from `asked`. For `get` of `https://github.com`, it answers with the account the worker was given, whatever
/// username the remote names. For a block, it says why on `said`, and tells git to stop, so git asks no other helper
/// and prompts for nothing. With nothing given, as once the worker has gone, it answers nothing. `store`, `erase` and
/// anything else git asks are only acknowledged, since what's given is the worker's to change. *Unconfirmed:* telling
/// git to stop, with `quit=1`.
pub fn answer_credential_request(
    dir: &Path,
    action: &str,
    asked: impl BufRead,
    mut answer: impl Write,
    mut said: impl Write,
) -> io::Result<()> {
    if action != "get" {
        return Ok(());
    }
    let asked = request(asked)?;
    if asked.get("protocol").map(String::as_str) != Some("https")
        || asked.get("host").map(String::as_str) != Some("github.com")
    {
        return Ok(());
    }
    let reason = match given(dir) {
        Ok(None | Some(GithubCredential::MachineOwn)) => return Ok(()),
        Ok(Some(GithubCredential::Account(credential))) => {
            let (username, password) = (credential.username.as_str(), credential.token.as_str());
            // Git reads a line at a time, so a value with a line in it would answer what the hub never sent.
            if [username, password].iter().all(|value| !value.contains(['\n', '\0'])) {
                return write!(answer, "username={username}\npassword={password}\n");
            }
            UNREADABLE.to_string()
        }
        Ok(Some(GithubCredential::Blocked { reason })) => reason,
        Ok(Some(GithubCredential::Unknown)) | Err(_) => UNREADABLE.to_string(),
    };
    writeln!(said, "{reason}")?;
    writeln!(answer, "quit=1")
}

/// What git asks, a `key=value` on each line until an empty one or the end.
fn request(asked: impl BufRead) -> io::Result<HashMap<String, String>> {
    let mut fields = HashMap::new();
    for line in asked.lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.to_string(), value.to_string());
        }
    }
    Ok(fields)
}

/// What the worker was given, or `None` once its files have gone.
fn given(dir: &Path) -> io::Result<Option<GithubCredential>> {
    let bytes = match std::fs::read(dir.join(ANSWER)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        read => read?,
    };
    serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other)
}
