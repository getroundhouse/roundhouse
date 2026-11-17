//! Finding who `gh` is logged in as, and the token it uses, against a stand-in for it on the `PATH`, which prints what
//! `gh` 2.103 prints in each case: a token's value only for `gh auth token`, as `gh auth status` prints none without
//! `--show-token`.

use std::path::Path;

use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GhLogin, GhLoginWorks, GithubToken};
use protocol::worker::FoundToken;

use super::find;
use crate::testing::given;

/// The login `gh` uses, logged in through its own config, as it prints it once GitHub has said its token works.
const ADA: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","scopes":"gist, read:org, repo","gitProtocol":"https"}]}}"#;
/// The same, when GitHub couldn't be reached to check it, or refused it.
const ADA_UNCHECKED: &str = r#"{"hosts":{"github.com":[{"state":"error","error":"dial tcp: connection refused","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","gitProtocol":"https"}]}}"#;
/// The same, for a token `gh` renews by itself.
const ADA_RENEWED_BY_GH: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","gitProtocol":"https","refreshable":true,"tokenExpiresAt":"2026-11-15T20:00:00Z"}]}}"#;
/// A token in its environment, whose owner GitHub said, which `gh` uses over its own login.
const GRACE_BY_TOKEN: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"grace","tokenSource":"GH_TOKEN","gitProtocol":"https"}]}}"#;
/// A token in its environment, whose owner GitHub didn't say, as for a token it refused.
const NOBODY_BY_TOKEN: &str = r#"{"hosts":{"github.com":[{"state":"error","error":"HTTP 401: Bad credentials","active":true,"host":"github.com","login":"","tokenSource":"GH_TOKEN","gitProtocol":"https"}]}}"#;

/// What `gh auth token` prints for a login whose token is `token`.
fn token_is(token: &str) -> String {
    format!("echo '{token}'")
}

/// What `gh auth token` prints when it has none, as for a login whose token was taken out of its keyring.
const NO_TOKEN: &str = "echo 'no oauth token found for github.com' >&2\nexit 1";

/// A stand-in for `gh` in `dir`, alone on the `PATH` it's given, that adds what it's asked to `asked`, then runs
/// `status` for `gh auth status` and `token` for `gh auth token`, which use only what the shell has built in. Anything
/// else fails, as asking GitHub itself would be a run the worker no longer makes.
fn gh(dir: &Path, status: &str, token: &str) -> Environment {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let asked = dir.join("asked").display().to_string();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{asked}'\ncase \"$1 $2\" in\n'auth status')\n{status}\n;;\n'auth token')\n{token}\n;;\n*)\nexit 9\n;;\nesac\n"
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    given().with("PATH", bin)
}

type Found = Result<(GhLogin, Option<FoundToken>), ApiError>;

/// What [`find`] makes of a stand-in that runs `status` and `token`, and what the stand-in was asked, a line a run.
async fn found(status: &str, token: &str) -> (Found, String) {
    found_given(status, token, |given| given).await
}

/// The same, with `also` adding to what the stand-in is given.
async fn found_given(status: &str, token: &str, also: impl FnOnce(Environment) -> Environment) -> (Found, String) {
    let dir = tempfile::tempdir().unwrap();
    let given = also(gh(dir.path(), status, token));

    let found = find(&given, dir.path()).await;

    (found, std::fs::read_to_string(dir.path().join("asked")).unwrap_or_default())
}

/// Ada's login, which the worker can't tell works, since that's the hub's to ask GitHub.
fn ada() -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::CouldntTell }
}

fn token(token: &str, renews: bool) -> FoundToken {
    FoundToken { token: GithubToken::new(token), renews }
}

#[tokio::test]
async fn a_login_is_who_gh_says_it_uses_for_github_with_the_token_it_prints_for_it() {
    let (found, asked) = found(&format!("echo '{ADA}'"), &token_is("gho_made_up")).await;

    let status = "auth status --active --hostname github.com --json hosts --no-refresh";
    assert_eq!(asked, format!("{status}\nauth token --hostname github.com --no-refresh\n"));
    assert_eq!(found.unwrap(), (ada(), Some(token("gho_made_up", false))));
}

/// However `gh` says the login stands, since whether GitHub takes the token is the hub's to ask.
#[tokio::test]
async fn a_login_gh_couldnt_check_is_found_with_its_token_all_the_same() {
    let (found, _) = found(&format!("echo '{ADA_UNCHECKED}'"), &token_is("gho_made_up")).await;

    assert_eq!(found.unwrap(), (ada(), Some(token("gho_made_up", false))));
}

/// A `gh` that fails to print it, one that prints nothing, and one that prints more than a token.
#[tokio::test]
async fn a_login_whose_token_gh_doesnt_print_is_found_without_one() {
    for printed in [NO_TOKEN.to_string(), "true".to_string(), "echo 'two words'".to_string()] {
        let (found, _) = found(&format!("echo '{ADA}'"), &printed).await;
        assert_eq!(found.unwrap(), (ada(), None), "for {printed}");
    }
}

/// Asking for the token isn't to renew it, as `gh` would if its time were up, and the hub hears that `gh` renews it.
#[tokio::test]
async fn a_token_gh_renews_by_itself_is_read_without_renewing_it_and_said_to_renew() {
    let (found, asked) = found(&format!("echo '{ADA_RENEWED_BY_GH}'"), &token_is("ghu_made_up")).await;

    assert_eq!(found.unwrap(), (ada(), Some(token("ghu_made_up", true))));
    assert!(asked.lines().all(|run| run.ends_with("--no-refresh")), "asked {asked}");
}

/// As a `gh` from 2.81 to before 2.103, which has no token it renews, says it.
#[tokio::test]
async fn a_gh_that_doesnt_know_no_refresh_is_asked_without_it() {
    let refuses = "case \"$*\" in *--no-refresh*) echo 'unknown flag: --no-refresh' >&2; exit 1;; esac";

    let (found, asked) = found(&format!("{refuses}\necho '{ADA}'"), &format!("{refuses}\necho gho_made_up")).await;

    assert_eq!(found.unwrap(), (ada(), Some(token("gho_made_up", false))));
    let (status, token) =
        ("auth status --active --hostname github.com --json hosts", "auth token --hostname github.com");
    assert_eq!(asked, format!("{status} --no-refresh\n{status}\n{token} --no-refresh\n{token}\n"));
}

/// As `gh` says it: on stderr, with a zero exit code, since it was asked for JSON.
#[tokio::test]
async fn a_gh_logged_in_to_nothing_is_logged_out() {
    let logged_out = r#"echo 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2
echo '{"hosts":{}}'"#;

    let (found, asked) = found(logged_out, &token_is("gho_made_up")).await;

    assert_eq!(found.unwrap(), (GhLogin::LoggedOut, None));
    assert!(!asked.contains("auth token"), "asked {asked}");
}

#[tokio::test]
async fn no_gh_on_the_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();

    let found = find(&given().with("PATH", dir.path()), dir.path()).await;

    assert_eq!(found.unwrap(), (GhLogin::NotInstalled, None));
}

/// A token from where `gh` runs is who it's logged in as, and the token it prints, as `gh` uses it over its own login,
/// so it's passed on to `gh` rather than taken out. One whose owner GitHub didn't say can't be told.
#[tokio::test]
async fn a_token_in_the_environment_is_who_github_says_its_owner_is() {
    let by_token = format!("[ \"$GH_TOKEN\" = made-up ] && {{ echo '{GRACE_BY_TOKEN}'; exit; }}\necho '{ADA}'");
    let printed = "echo \"$GH_TOKEN\"";

    let (grace, _) = found_given(&by_token, printed, |given| given.with("GH_TOKEN", "made-up")).await;
    let (nobody, _) = found(&format!("echo '{NOBODY_BY_TOKEN}'"), printed).await;

    let login = GhLogin::LoggedIn { username: "grace".into(), works: GhLoginWorks::CouldntTell };
    assert_eq!(grace.unwrap(), (login, Some(token("made-up", false))));
    assert_eq!(nobody.map_err(|e| e.code), Err(ErrorCode::Worker));
}

/// Something other than what `gh` prints, a `gh` older than 2.81, which doesn't know `--json` here, and one that fails.
#[tokio::test]
async fn what_cant_be_read_is_refused_rather_than_taken_for_logged_out() {
    let printed = token_is("gho_made_up");
    let (garbage, _) = found("echo 'Logged in to github.com account ada (keyring)'", &printed).await;
    let (older, _) = found("echo 'unknown flag: --json' >&2\nexit 1", &printed).await;
    let (failed, _) = found(&format!("echo '{ADA}'\nexit 2"), &printed).await;

    for found in [garbage, older, failed] {
        assert_eq!(found.map_err(|e| e.code), Err(ErrorCode::Worker));
    }
}
