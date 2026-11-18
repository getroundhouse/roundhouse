use std::collections::HashMap;
use std::path::Path;
use std::process::Output;

use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GhLogin, GhLoginWorks};
use protocol::worker::{FoundToken, GH_RUNS_WITHIN, GithubLoginReply, WorkerResponse};
use serde::Deserialize;
use tokio::time::Instant;

use super::token::token;
use crate::gh::{Unran, run_gh};

/// What `gh` is asked: its login for github.com that it uses, in JSON, which `gh` 2.81 is the first to print, and which
/// it prints with a zero exit code however the login stands. And not to renew a token it renews by itself, as `gh` 2.103
/// would, which rotates what renews it, and forgets the login when GitHub refuses that, so reading it can't change it.
const ASKED: [&str; 8] = ["auth", "status", "--active", "--hostname", "github.com", "--json", "hosts", "--no-refresh"];

/// [`ASKED`] without `--no-refresh`, for a `gh` older than the one that has it, which has nothing to renew.
const ASKED_OF_AN_OLDER_GH: [&str; 7] = ["auth", "status", "--active", "--hostname", "github.com", "--json", "hosts"];

/// [`FindGithubLogin`](protocol::worker::FindGithubLogin)'s answer, read in `dir`, with `machine`, what the worker gives
/// a process under the machine's own, whatever git credential the hub has set, since it's the machine's own login
/// that's found.
pub(crate) async fn reply(machine: &Environment, dir: &Path) -> Result<WorkerResponse, ApiError> {
    let (login, token) = find(machine, dir).await?;
    Ok(WorkerResponse::GithubLogin(GithubLoginReply { login, token }))
}

/// Who `gh` is logged in to github.com as, run in `dir` with `given`, and the token it uses for that login, as
/// [`FindGithubLogin`](protocol::worker::FindGithubLogin) says. Every run of `gh` for it shares [`GH_RUNS_WITHIN`].
pub(super) async fn find(given: &Environment, dir: &Path) -> Result<(GhLogin, Option<FoundToken>), ApiError> {
    let until = Instant::now() + GH_RUNS_WITHIN;
    let Some(printed) = status(given, dir, until).await? else { return Ok((GhLogin::NotInstalled, None)) };
    let Read::LoggedIn { username, refreshable } =
        read(&printed.stdout).ok_or_else(|| couldnt_tell("gh printed something else, or named nobody"))?
    else {
        return Ok((GhLogin::LoggedOut, None));
    };
    let token = token(until.saturating_duration_since(Instant::now()), given, dir).await;
    // Whether it works is the hub's to ask GitHub, with the token.
    let login = GhLogin::LoggedIn { username: username.into(), works: GhLoginWorks::CouldntTell };
    Ok((login, token.map(|token| FoundToken { token, renews: refreshable })))
}

/// What `gh auth status` printed, asked as [`ASKED`], or as [`ASKED_OF_AN_OLDER_GH`] once it has said it doesn't know
/// `--no-refresh`, or `None` when there's no `gh`.
async fn status(given: &Environment, dir: &Path, until: Instant) -> Result<Option<Output>, ApiError> {
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
        Err(Unran::Failed(why)) => return Err(couldnt_tell(&why)),
    };
    if !printed.status.success() {
        // Why, in this worker's words rather than what `gh` printed, which can name the person's own login.
        let why = if refused(&printed, "--json") {
            "gh is older than 2.81, the first to say".into()
        } else {
            exited(&printed)
        };
        return Err(couldnt_tell(&why));
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
fn read(printed: &[u8]) -> Option<Read> {
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

fn couldnt_tell(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("couldn't tell who gh is logged in to github.com as: {why}"))
}
