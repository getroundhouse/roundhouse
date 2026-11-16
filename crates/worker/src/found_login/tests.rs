//! Finding who `gh` is logged in as, and whether GitHub takes its token, against a stand-in for it on the `PATH`,
//! which prints what `gh` 2.103 prints in each case, with a token's value nowhere, as `gh` prints none without
//! `--show-token`.

use std::path::Path;

use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GhLogin, GhLoginWorks};

use super::find;

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

/// What `gh api --include` prints for GitHub's answer with `status_line` and `body`, and how it exits: the status line
/// ending with `\n`, the headers and the blank line after them with `\r\n`, as `gh` 2.103 prints them.
fn answering(status_line: &str, body: &str, exit: u8) -> String {
    let head =
        format!("{status_line}\\nContent-Type: application/json; charset=utf-8\\r\\nX-Oauth-Scopes: repo\\r\\n\\r\\n");
    format!("printf '{head}%s\\n' '{body}'\nexit {exit}")
}

fn github_names(login: &str) -> String {
    answering("HTTP/2.0 200 OK", &format!(r#"{{"login":"{login}","id":1,"type":"User"}}"#), 0)
}

fn github_refuses() -> String {
    let refused = answering("HTTP/2.0 401 Unauthorized", r#"{"message":"Bad credentials","status":"401"}"#, 1);
    format!("echo 'gh: Bad credentials (HTTP 401)' >&2\n{refused}")
}

/// What `gh` prints when it gets no answer from GitHub.
const UNREACHED: &str = "echo 'Get \"https://api.github.com/user\": dial tcp: connect: connection refused' >&2\nexit 1";

/// A stand-in for `gh` in `dir`, alone on the `PATH` it's given, that adds what it's asked to `asked`, then runs
/// `status` for `gh auth status` and `api` for `gh api`, which use only what the shell has built in.
fn gh(dir: &Path, status: &str, api: &str) -> Environment {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let asked = dir.join("asked").display().to_string();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{asked}'\ncase \"$1\" in\nauth)\n{status}\n;;\napi)\n{api}\n;;\nesac\n"
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    Environment::NONE.with("PATH", bin)
}

/// What [`find`] makes of a stand-in that runs `status` and `api`, and what the stand-in was asked, a line a run.
async fn found(status: &str, api: &str) -> (Result<GhLogin, ApiError>, String) {
    found_given(status, api, |given| given).await
}

/// The same, with `also` adding to what the stand-in is given.
async fn found_given(
    status: &str,
    api: &str,
    also: impl FnOnce(Environment) -> Environment,
) -> (Result<GhLogin, ApiError>, String) {
    let dir = tempfile::tempdir().unwrap();
    let given = also(gh(dir.path(), status, api));

    let found = find(&given, dir.path()).await;

    (found, std::fs::read_to_string(dir.path().join("asked")).unwrap_or_default())
}

fn ada(works: GhLoginWorks) -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works }
}

#[tokio::test]
async fn a_login_is_who_gh_says_it_uses_for_github_and_works_once_github_names_it_for_its_token() {
    let (found, asked) = found(&format!("echo '{ADA}'"), &github_names("ada")).await;

    let status = "auth status --active --hostname github.com --json hosts --no-refresh";
    assert_eq!(asked, format!("{status}\napi --include user\n"));
    assert_eq!(found.unwrap(), ada(GhLoginWorks::Yes));
}

#[tokio::test]
async fn a_login_whose_token_github_refuses_is_refused() {
    let (found, _) = found(&format!("echo '{ADA_UNCHECKED}'"), &github_refuses()).await;

    assert_eq!(found.unwrap(), ada(GhLoginWorks::Refused));
}

/// No answer from GitHub, as when it can't be reached, any status but a success or a refusal, and a success naming
/// another user or nobody. Each still the login `gh` names.
#[tokio::test]
async fn whether_a_login_works_cant_be_told_from_anything_else() {
    let answers = [
        UNREACHED.to_string(),
        answering("HTTP/2.0 403 Forbidden", r#"{"message":"Resource not accessible"}"#, 1),
        answering("HTTP/2.0 502 Bad Gateway", "<html></html>", 1),
        github_names("grace"),
        answering("HTTP/2.0 200 OK", r#"{"id":1}"#, 0),
        "echo '{\"login\":\"ada\"}'".to_string(),
    ];

    for api in answers {
        let (found, _) = found(&format!("echo '{ADA_UNCHECKED}'"), &api).await;
        assert_eq!(found.unwrap(), ada(GhLoginWorks::CouldntTell), "for {api}");
    }
}

/// As GitHub's usernames are, and as GitHub may write it otherwise than `gh` kept it.
#[tokio::test]
async fn github_naming_the_login_in_another_case_is_the_same_login() {
    let (found, _) = found(&format!("echo '{ADA}'"), &github_names("Ada")).await;

    assert_eq!(found.unwrap(), ada(GhLoginWorks::Yes));
}

/// Asking GitHub with a token `gh` renews by itself would renew it if it's due, so it isn't asked.
#[tokio::test]
async fn a_token_gh_renews_by_itself_isnt_checked() {
    let (found, asked) = found(&format!("echo '{ADA_RENEWED_BY_GH}'"), &github_names("ada")).await;

    assert_eq!(found.unwrap(), ada(GhLoginWorks::CouldntTell));
    assert!(!asked.contains("api"), "asked {asked}");
}

/// As a `gh` from 2.81 to before 2.103, which has no token it renews, says it.
#[tokio::test]
async fn a_gh_that_doesnt_know_no_refresh_is_asked_without_it() {
    let older =
        format!("case \"$*\" in *--no-refresh*) echo 'unknown flag: --no-refresh' >&2; exit 1;; esac\necho '{ADA}'");

    let (found, asked) = found(&older, &github_names("ada")).await;

    assert_eq!(found.unwrap(), ada(GhLoginWorks::Yes));
    let without = "auth status --active --hostname github.com --json hosts";
    assert_eq!(asked, format!("{without} --no-refresh\n{without}\napi --include user\n"));
}

/// As `gh` says it: on stderr, with a zero exit code, since it was asked for JSON.
#[tokio::test]
async fn a_gh_logged_in_to_nothing_is_logged_out() {
    let logged_out = r#"echo 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2
echo '{"hosts":{}}'"#;

    let (found, _) = found(logged_out, &github_names("ada")).await;

    assert_eq!(found.unwrap(), GhLogin::LoggedOut);
}

#[tokio::test]
async fn no_gh_on_the_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();

    let found = find(&Environment::NONE.with("PATH", dir.path()), dir.path()).await;

    assert_eq!(found.unwrap(), GhLogin::NotInstalled);
}

/// A token from where `gh` runs is who it's logged in as, as `gh` uses it over its own login, so it's passed on to
/// `gh` rather than taken out. One whose owner GitHub didn't say can't be told.
#[tokio::test]
async fn a_token_in_the_environment_is_who_github_says_its_owner_is() {
    let by_token = format!("[ \"$GH_TOKEN\" = made-up ] && {{ echo '{GRACE_BY_TOKEN}'; exit; }}\necho '{ADA}'");
    let checked = format!("[ \"$GH_TOKEN\" = made-up ] || exit 9\n{}", github_names("grace"));

    let (grace, _) = found_given(&by_token, &checked, |given| given.with("GH_TOKEN", "made-up")).await;
    let (nobody, _) = found(&format!("echo '{NOBODY_BY_TOKEN}'"), &github_refuses()).await;

    assert_eq!(grace.unwrap(), GhLogin::LoggedIn { username: "grace".into(), works: GhLoginWorks::Yes });
    assert_eq!(nobody.map_err(|e| e.code), Err(ErrorCode::Worker));
}

/// Something other than what `gh` prints, a `gh` older than 2.81, which doesn't know `--json` here, and one that fails.
#[tokio::test]
async fn what_cant_be_read_is_refused_rather_than_taken_for_logged_out() {
    let api = github_names("ada");
    let (garbage, _) = found("echo 'Logged in to github.com account ada (keyring)'", &api).await;
    let (older, _) = found("echo 'unknown flag: --json' >&2\nexit 1", &api).await;
    let (failed, _) = found(&format!("echo '{ADA}'\nexit 2"), &api).await;

    for found in [garbage, older, failed] {
        assert_eq!(found.map_err(|e| e.code), Err(ErrorCode::Worker));
    }
}
