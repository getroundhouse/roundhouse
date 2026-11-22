use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use process::Environment;
use protocol::model::{GithubToken, GithubUsername};
use serde::Deserialize;
use tokio::time::Instant;

use super::running::{Unran, run_gh};
use super::token::token;
use crate::machine_environment::MachineEnvironment;

/// How long the hub lets each read of `gh` take, every run of `gh` for it together, before it stops it, so a `gh` that
/// hangs holds up only the read.
const READ_WITHIN: Duration = Duration::from_secs(15);

/// The tokens for github.com `gh` takes from its environment over its own login.
const TOKENS: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// What `gh` is asked: its login for github.com that it uses, in JSON, which `gh` 2.81 is the first to print, and which
/// it prints with a zero exit code however the login stands. And not to renew a token it renews by itself, as `gh` 2.103
/// would, which rotates what renews it, and forgets the login when GitHub refuses that, so reading it can't change it.
const ASKED: [&str; 8] = ["auth", "status", "--active", "--hostname", "github.com", "--json", "hosts", "--no-refresh"];

/// [`ASKED`] without `--no-refresh`, for a `gh` older than the one that has it, which has nothing to renew.
const ASKED_OF_AN_OLDER_GH: [&str; 7] = ["auth", "status", "--active", "--hostname", "github.com", "--json", "hosts"];

/// What `gh` on the hub's machine is logged in to github.com with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GhRead {
    /// Logged in as `username`, as `gh`'s own config names it. `token` is what it uses for it, unless it printed none,
    /// and `renews` whether `gh` renews that token by itself.
    LoggedIn { username: GithubUsername, token: Option<GithubToken>, renews: bool },
    /// `gh` is there, and logged in to nothing for github.com.
    LoggedOut,
    /// There's no `gh` on the machine's `PATH`.
    NotInstalled,
}

/// Why the hub couldn't tell who `gh` is logged in as.
#[derive(Debug, thiserror::Error)]
#[error("couldn't tell who gh is logged in to github.com as: {0}")]
pub(crate) struct GhUnread(String);

/// `gh` on the hub's machine: the machine's environment, whose `PATH` it's found on, and where it's run.
pub(crate) struct Gh {
    machine: MachineEnvironment,
    dir: PathBuf,
}

impl Gh {
    /// `gh` run in `machine`, and in `dir`, which is the hub's data directory, so it's in no repository.
    pub(crate) fn new(machine: MachineEnvironment, dir: PathBuf) -> Self {
        Self { machine, dir }
    }

    /// Who `gh` is logged in to github.com as, and the token it uses for that login. Every run of `gh` for it shares
    /// [`READ_WITHIN`]. Refused when `gh` took longer than that, failed, as one older than 2.81 does, since 2.81 is the
    /// first to say in JSON, printed what the hub can't read, or named nobody, as for a token in the environment whose
    /// owner GitHub didn't say.
    pub(crate) async fn read(&self) -> Result<GhRead, GhUnread> {
        let until = Instant::now() + READ_WITHIN;
        let given = self.given().await;
        let Some(printed) = status(&given, &self.dir, until).await? else { return Ok(GhRead::NotInstalled) };
        let Read::LoggedIn { username, refreshable } =
            parse(&printed.stdout).ok_or_else(|| GhUnread("gh printed something else, or named nobody".into()))?
        else {
            return Ok(GhRead::LoggedOut);
        };
        let token = token(until.saturating_duration_since(Instant::now()), &given, &self.dir, false).await;
        Ok(GhRead::LoggedIn { username: username.into(), token, renews: refreshable })
    }

    /// The token `gh` uses for github.com once it has renewed one it renews by itself whose time is up, as any run of
    /// `gh` the person makes would, for a token GitHub refused. `None` when it printed none.
    pub(crate) async fn renewed(&self) -> Option<GithubToken> {
        token(READ_WITHIN, &self.given().await, &self.dir, true).await
    }

    /// What `gh` is given: the machine's environment, which is the profile's, so a `gh` installed with Homebrew is
    /// found by a hub an app started. And none of [`TOKENS`], which `gh` would use over its own login, so what's read
    /// is the login `gh auth login` and `gh auth switch` change, as a block says to run, even when the profile exports
    /// one.
    pub(super) async fn given(&self) -> Environment {
        TOKENS.into_iter().fold(self.machine.given(&[]).await, Environment::without)
    }
}

/// What `gh auth status` printed, asked as [`ASKED`], or as [`ASKED_OF_AN_OLDER_GH`] once it has said it doesn't know
/// `--no-refresh`, or `None` when there's no `gh`.
async fn status(given: &Environment, dir: &Path, until: Instant) -> Result<Option<Output>, GhUnread> {
    let run = |args: &'static [&'static str]| {
        let within = until.saturating_duration_since(Instant::now());
        async move { run_gh(within, given, dir, args).await }
    };
    let printed = match run(&ASKED).await {
        Ok(printed) if refused(&printed, "--no-refresh") => run(&ASKED_OF_AN_OLDER_GH).await,
        ran => ran,
    };
    let printed = match printed {
        Ok(printed) => printed,
        Err(Unran::Missing) => return Ok(None),
        Err(Unran::Failed(why)) => return Err(GhUnread(why)),
    };
    if !printed.status.success() {
        // Why, in the hub's words rather than what `gh` printed, which can name the person's own login.
        let why = if refused(&printed, "--json") {
            "gh is older than 2.81, the first to say".into()
        } else {
            exited(&printed)
        };
        return Err(GhUnread(why));
    }
    Ok(Some(printed))
}

/// Whether `gh` failed for not knowing `flag`, as it says on stderr.
fn refused(printed: &Output, flag: &str) -> bool {
    !printed.status.success() && String::from_utf8_lossy(&printed.stderr).contains(&format!("unknown flag: {flag}"))
}

/// What `gh auth status --json hosts` prints, as far as it's read: who each login is, which it uses, and whether `gh`
/// renews its token, so nothing else it prints, such as a token, which `--show-token` would add, is kept even while
/// it's read.
#[derive(Deserialize)]
struct Status {
    hosts: HashMap<String, Vec<Entry>>,
}

/// One of a host's logins, which `gh` calls an entry.
#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    active: bool,
    /// The username, which `gh` leaves empty for a token from where it runs whose owner GitHub didn't say.
    #[serde(default)]
    login: String,
    /// Whether it's a token `gh` renews by itself, which `gh` 2.103 is the first to have, and to say.
    #[serde(default)]
    refreshable: bool,
}

enum Read {
    LoggedIn { username: String, refreshable: bool },
    LoggedOut,
}

/// `None` when it isn't what `gh` prints, or names nobody for the login it uses.
fn parse(printed: &[u8]) -> Option<Read> {
    let status: Status = serde_json::from_slice(printed).ok()?;
    let Some(logins) = status.hosts.get("github.com").filter(|logins| !logins.is_empty()) else {
        return Some(Read::LoggedOut);
    };
    let used = logins.iter().find(|login| login.active)?;
    (!used.login.is_empty()).then(|| Read::LoggedIn { username: used.login.clone(), refreshable: used.refreshable })
}

fn exited(printed: &Output) -> String {
    match printed.status.code() {
        Some(code) => format!("gh failed with exit code {code}"),
        None => "gh was stopped".to_string(),
    }
}
