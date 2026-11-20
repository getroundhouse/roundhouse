//! The files the worker hands its processes for github.com, in a directory of its state directory that only its user
//! can open: a git config file, who commits are by in a repository on github.com, a `gh` config directory, and what
//! its credential helper answers with.

use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use process::Environment;
use protocol::model::GithubToken;
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};

/// The directory in the worker's state directory that holds them.
const DIR: &str = "git-credential";
/// The git config file every process is given, which git reads again on every run.
const CONFIG: &str = "config";
/// Who commits are by, which the git config includes only in a repository on github.com, and which is there only while
/// the credential has a commit identity. The git config names it by this alone, which git reads as a file beside it.
const IDENTITY: &str = "identity";
/// What the credential helper answers with, which is the credential as the hub sent it. A file of its own, so the token
/// isn't in git's config, where `git config --list` would print it. *Unconfirmed.*
pub(super) const ANSWER: &str = "credential.json";
/// The `gh` config directory every process is given, and the file in it where `gh` keeps its logins.
const GH: &str = "gh";
const HOSTS: &str = "hosts.yml";

/// Who `gh` is logged in to github.com as while the credential is blocked, with a token GitHub refuses. `gh` takes a
/// token in its `hosts.yml` before the one it keeps in the system's keychain, and goes on to that one when the file
/// holds none, as `gh` 2.103.3 does, so a block that logged it in to nothing would leave it the machine's own.
const BLOCKED_USER: &str = "blocked-by-roundhouse";
const BLOCKED_TOKEN: &str = "blocked-by-roundhouse";

/// Taken out of what every process given the files inherits, since `gh` takes any of them over its config directory.
const GH_TOKENS: [&str; 4] = ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"];

/// Where the git config sends GitHub's SSH addresses, so a push to one goes through the helper.
const SENT_TO: &str = "https://github.com/";

/// How GitHub's SSH addresses start, which the git config sends to [`SENT_TO`], so a remote over SSH goes through the
/// helper too, rather than push with the machine's key. Not `ssh://github.com/` or one with a port, which git takes
/// for others.
const GITHUB_OVER_SSH: [&str; 2] = ["git@github.com:", "ssh://git@github.com/"];

/// How the address of a repository on github.com starts over HTTPS, as a pattern of git's `includeIf`: without a user
/// or with one, since git asks the helper for `https://github.com` whatever user an address names.
const GITHUB_OVER_HTTPS: [&str; 2] = ["https://github.com/", "https://*@github.com/"];

/// What a process can be handed for github.com: an account to push as, or a block, with what to do about it.
pub(super) enum Handed {
    Account(GitCredential),
    Blocked(String),
}

/// Where the files are, and the helper the git config names.
pub(super) struct Files {
    dir: PathBuf,
    /// The helper's program and its arguments, after which the helper is given the directory, and git the action.
    helper: Vec<OsString>,
}

impl Files {
    pub(super) fn new(state_dir: &Path, helper: Vec<OsString>) -> Self {
        Self { dir: state_dir.join(DIR), helper }
    }

    /// What a process is given on a machine whose own is `machine`, so its `git` and `gh` read the files: the git
    /// config as an `include.path` given in `GIT_CONFIG_*`, which replaces any of those the worker inherited, and the
    /// `gh` directory as `GH_CONFIG_DIR`. *Unconfirmed:* replacing what it inherited.
    pub(super) fn environment(&self, machine: &Environment) -> Environment {
        GH_TOKENS
            .into_iter()
            .fold(machine.clone(), Environment::without)
            .with("GIT_CONFIG_COUNT", "1")
            .with("GIT_CONFIG_KEY_0", "include.path")
            .with("GIT_CONFIG_VALUE_0", self.dir.join(CONFIG))
            .with("GH_CONFIG_DIR", self.dir.join(GH))
    }

    /// Rewrites each file for `handed`, the helper's answer, `gh`'s logins and who commits are by before the git config
    /// that sends git to them. Each is replaced whole, by renaming a new one over it, so a process reading one meanwhile
    /// reads the last or the next, never part of either. Who commits are by is removed for a credential without a
    /// commit identity, and for a block, and git passes over an include of a file that isn't there. None is synced,
    /// since the worker removes them all as it starts, so nothing reads what a power cut leaves. *Unconfirmed:* not
    /// syncing.
    pub(super) fn write(&self, handed: &Handed) -> io::Result<()> {
        let gh = self.dir.join(GH);
        private_files::create_dir_all(&gh)?;
        let answer = match handed {
            Handed::Account(credential) => GithubCredential::Account(credential.clone()),
            Handed::Blocked(reason) => GithubCredential::Blocked { reason: reason.clone() },
        };
        replace(&self.dir.join(ANSWER), &serde_json::to_vec(&answer).map_err(io::Error::other)?)?;
        let identity = match handed {
            Handed::Account(credential) => {
                replace(&gh.join(HOSTS), hosts(credential).as_bytes())?;
                credential.identity.as_ref()
            }
            Handed::Blocked(_) => {
                let refused = GitCredential {
                    username: BLOCKED_USER.into(),
                    token: GithubToken::new(BLOCKED_TOKEN),
                    identity: None,
                };
                replace(&gh.join(HOSTS), hosts(&refused).as_bytes())?;
                None
            }
        };
        match identity {
            Some(identity) => replace(&self.dir.join(IDENTITY), &identity_config(identity))?,
            None => remove_file(&self.dir.join(IDENTITY))?,
        }
        replace(&self.dir.join(CONFIG), &self.config())
    }

    /// Removes the directory and everything in it, the git config first, so a process that reads it meanwhile goes
    /// on with the machine's own. Nothing to do once it has gone.
    pub(super) fn remove(&self) -> io::Result<()> {
        remove_file(&self.dir.join(CONFIG))?;
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        }
    }

    /// The machine's helpers for `https://github.com` reset, then the worker's own, and GitHub's SSH addresses sent
    /// over HTTPS, so a remote over SSH goes through the helper too. And who commits are by, included only in a
    /// repository with a remote on github.com, since the helper is only for github.com, so one elsewhere, such as on
    /// GitLab or GitHub Enterprise, commits as its own config or the machine's says. So does every repository under a
    /// git before 2.36, the first to know `hasconfig:remote.*.url`, since git takes a condition it doesn't know as
    /// false. It's the same for every account and block, since what the helper answers says which, and whether who
    /// commits are by is there says whether there's anyone. `docs/reference/git.md` has what git does with the
    /// condition. *Unconfirmed:* any remote counting, not only `origin`, and what a git before 2.36 does.
    fn config(&self) -> Vec<u8> {
        let words =
            self.helper.iter().map(|word| word.as_encoded_bytes()).chain([self.dir.as_os_str().as_encoded_bytes()]);
        let mut helper = b"!".to_vec();
        for (i, word) in words.enumerate() {
            if i > 0 {
                helper.push(b' ');
            }
            helper.extend(shell_quoted(word));
        }
        let mut config = b"# Rewritten by the Roundhouse worker whenever its hub sets the git credential.\n".to_vec();
        config.extend(b"[credential \"https://github.com\"]\n\thelper =\n\thelper = ");
        config.extend(config_quoted(&helper));
        config.extend(format!("\n[url \"{SENT_TO}\"]\n").as_bytes());
        for start in GITHUB_OVER_SSH {
            config.extend(format!("\tinsteadOf = {start}\n").as_bytes());
        }
        // git matches the address of each remote as its config has it, before any `insteadOf`, so each of GitHub's is
        // named. After how it starts, `*` takes the owner, since it takes no `/`, and `**` the rest, so
        // `git@github.com:**` alone would take only an address with no `/` in it.
        for start in GITHUB_OVER_HTTPS.into_iter().chain(GITHUB_OVER_SSH) {
            let include = format!("[includeIf \"hasconfig:remote.*.url:{start}*/**\"]\n\tpath = {IDENTITY}\n");
            config.extend(include.as_bytes());
        }
        config
    }
}

/// Who commits are by, as git's own `user.name` and `user.email`, which the git config's include puts in the command's
/// own config, so git takes them over the repository's and the person's own. Nothing else, since every git would fail
/// on a remote's address in a file it includes for `hasconfig:remote.*.url`, as git 2.54 does.
fn identity_config(identity: &GitIdentity) -> Vec<u8> {
    let mut config = b"# Rewritten by the Roundhouse worker whenever its hub sets the git credential.\n".to_vec();
    config.extend(b"[user]\n\tname = ");
    config.extend(config_quoted(identity.name.as_bytes()));
    config.extend(b"\n\temail = ");
    config.extend(config_quoted(identity.email.as_bytes()));
    config.push(b'\n');
    config
}

/// `gh`'s logins, as `gh` writes them with the token in the file, logged in to github.com as `credential`. Each value
/// is quoted as JSON is, which YAML reads the same. Its user is the username git gives GitHub, so for an app's
/// installation `gh` is logged in as `x-access-token`, and not as the app's bot. *Unconfirmed.*
fn hosts(credential: &GitCredential) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    let (user, token) = (quoted(credential.username.as_str()), quoted(credential.token.as_str()));
    format!(
        "github.com:\n    users:\n        {user}:\n            oauth_token: {token}\n    git_protocol: https\n    \
         oauth_token: {token}\n    user: {user}\n"
    )
}

/// `word` as one word to the shell git runs a helper with.
fn shell_quoted(word: &[u8]) -> Vec<u8> {
    let mut quoted = b"'".to_vec();
    for &byte in word {
        if byte == b'\'' {
            quoted.extend(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    quoted
}

/// `value` as a git config file holds it, in double quotes, so nothing in it reads as a comment or ends the line.
fn config_quoted(value: &[u8]) -> Vec<u8> {
    let mut quoted = b"\"".to_vec();
    for &byte in value {
        match byte {
            b'"' | b'\\' => quoted.extend([b'\\', byte]),
            b'\n' => quoted.extend(b"\\n"),
            b'\t' => quoted.extend(b"\\t"),
            _ => quoted.push(byte),
        }
    }
    quoted.push(b'"');
    quoted
}

/// Replaces `path` with `bytes` all at once: a new file, only this user's, renamed over it.
fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    private_files::file_options().write(true).create(true).truncate(true).open(&partial)?.write_all(bytes)?;
    std::fs::rename(&partial, path)
}

fn remove_file(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}
