use std::path::Path;
use std::time::Duration;

use process::Environment;
use protocol::model::GithubToken;

use crate::gh::run_gh_within;

/// What `gh` is asked: the token it uses for github.com's login it uses, and not to renew one it renews by itself, as it
/// would if its time is up, which rotates what renews it, and forgets the login when GitHub refuses that, so reading it
/// can't change it.
const ASKED: [&str; 5] = ["auth", "token", "--hostname", "github.com", "--no-refresh"];

/// [`ASKED`] without `--no-refresh`, for a `gh` older than the one that has it, which has nothing to renew.
const ASKED_OF_AN_OLDER_GH: [&str; 4] = ["auth", "token", "--hostname", "github.com"];

/// The token `gh` uses for github.com, asked within `within`, or `None` when it didn't print one, as when it failed or
/// ran out of time. A token from where it runs, such as `GH_TOKEN`, is the one it uses over its own login, so it's the
/// one printed.
pub(super) async fn token(within: Duration, given: &Environment, dir: &Path) -> Option<GithubToken> {
    let until = tokio::time::Instant::now() + within;
    let run = |args: &'static [&'static str]| {
        let within = until.saturating_duration_since(tokio::time::Instant::now());
        async move { run_gh_within(within, given, dir, args).await.ok() }
    };
    let mut printed = run(&ASKED).await?;
    if !printed.status.success() && String::from_utf8_lossy(&printed.stderr).contains("unknown flag: --no-refresh") {
        printed = run(&ASKED_OF_AN_OLDER_GH).await?;
    }
    if !printed.status.success() {
        return None;
    }
    let token = String::from_utf8(printed.stdout).ok()?.trim().to_string();
    (!token.is_empty() && !token.contains(char::is_whitespace)).then(|| GithubToken::new(token))
}
