use std::path::Path;
use std::time::Duration;

use process::Environment;
use protocol::model::GithubToken;

use super::running::run_gh;

/// What `gh` is asked: the token it uses for github.com's login it uses, and not to renew one it renews by itself, as it
/// would if its time is up, which rotates what renews it, and forgets the login when GitHub refuses that, so reading it
/// changes nothing of the person's `gh`.
const ASKED: [&str; 5] = ["auth", "token", "--hostname", "github.com", "--no-refresh"];

/// [`ASKED`] without `--no-refresh`, for a `gh` older than the one that has it, which has nothing to renew, and to have
/// `gh` renew a token whose time is up, as any run of `gh` the person makes would.
const RENEWING: [&str; 4] = ["auth", "token", "--hostname", "github.com"];

/// The token `gh` uses for github.com, asked within `within`, or `None` when it didn't print one, as when it failed or
/// ran out of time. With `renewing`, `gh` renews a token it renews by itself whose time is up, as it would for the person.
pub(super) async fn token(within: Duration, given: &Environment, dir: &Path, renewing: bool) -> Option<GithubToken> {
    let until = tokio::time::Instant::now() + within;
    let run = |args: &'static [&'static str]| {
        let within = until.saturating_duration_since(tokio::time::Instant::now());
        async move { run_gh(within, given, dir, args).await.ok() }
    };
    let mut printed = run(if renewing { &RENEWING } else { &ASKED }).await?;
    if !printed.status.success() && String::from_utf8_lossy(&printed.stderr).contains("unknown flag: --no-refresh") {
        printed = run(&RENEWING).await?;
    }
    if !printed.status.success() {
        return None;
    }
    let token = String::from_utf8(printed.stdout).ok()?.trim().to_string();
    (!token.is_empty() && !token.contains(char::is_whitespace)).then(|| GithubToken::new(token))
}
