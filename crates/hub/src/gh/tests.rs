//! Reading who `gh` is logged in as, and the token it uses, against a stand-in for it on the `PATH`, which prints what
//! `gh` 2.103 prints in each case: a token's value only for `gh auth token`, as `gh auth status` prints none without
//! `--show-token`.

use std::path::Path;

use protocol::model::GithubToken;

use super::reading::GhUnread;
use super::{Gh, GhRead, GhTokens};
use crate::machine_environment::MachineEnvironment;

/// The login `gh` uses, logged in through its own config, as it prints it once GitHub has said its token works.
const ADA: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","scopes":"gist, read:org, repo","gitProtocol":"https"}]}}"#;
/// The same, when GitHub couldn't be reached to check it, or refused it.
const ADA_UNCHECKED: &str = r#"{"hosts":{"github.com":[{"state":"error","error":"dial tcp: connection refused","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","gitProtocol":"https"}]}}"#;
/// The same, for a token `gh` renews by itself.
const ADA_RENEWED_BY_GH: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ada","tokenSource":"keyring","gitProtocol":"https","refreshable":true,"tokenExpiresAt":"2026-11-15T20:00:00Z"}]}}"#;
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
/// else fails.
fn gh(dir: &Path, status: &str, token: &str) -> MachineEnvironment {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let asked = dir.join("asked").display().to_string();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{asked}'\ncase \"$1 $2\" in\n'auth status')\n{status}\n;;\n'auth token')\n{token}\n;;\n*)\nexit 9\n;;\nesac\n"
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    MachineEnvironment::only(&[("PATH", &bin.display().to_string())])
}

/// What [`read`] makes of a stand-in that runs `status` and `token`, and what the stand-in was asked, a line a run.
async fn found(status: &str, token: &str) -> (Result<GhRead, GhUnread>, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = gh(dir.path(), status, token);

    let found = Gh::new(path, dir.path().to_path_buf()).read(GhTokens::TakenOut).await;

    (found, std::fs::read_to_string(dir.path().join("asked")).unwrap_or_default())
}

fn ada(token: Option<&str>, renews: bool) -> GhRead {
    GhRead::LoggedIn { username: "ada".into(), token: token.map(GithubToken::new), renews }
}

#[tokio::test]
async fn a_login_is_who_gh_says_it_uses_for_github_with_the_token_it_prints_for_it() {
    let (found, asked) = found(&format!("echo '{ADA}'"), &token_is("gho_made_up")).await;

    let status = "auth status --active --hostname github.com --json hosts --no-refresh";
    assert_eq!(asked, format!("{status}\nauth token --hostname github.com --no-refresh\n"));
    assert_eq!(found.unwrap(), ada(Some("gho_made_up"), false));
}

/// However `gh` says the login stands, since whether GitHub takes the token is the hub's to ask.
#[tokio::test]
async fn a_login_gh_couldnt_check_is_read_with_its_token_all_the_same() {
    let (found, _) = found(&format!("echo '{ADA_UNCHECKED}'"), &token_is("gho_made_up")).await;

    assert_eq!(found.unwrap(), ada(Some("gho_made_up"), false));
}

/// A `gh` that fails to print it, one that prints nothing, and one that prints more than a token.
#[tokio::test]
async fn a_login_whose_token_gh_doesnt_print_is_read_without_one() {
    for printed in [NO_TOKEN.to_string(), "true".to_string(), "echo 'two words'".to_string()] {
        let (found, _) = found(&format!("echo '{ADA}'"), &printed).await;
        assert_eq!(found.unwrap(), ada(None, false), "for {printed}");
    }
}

/// Asking for the token isn't to renew it, as `gh` would if its time were up, and the hub hears that `gh` renews it.
#[tokio::test]
async fn a_token_gh_renews_by_itself_is_read_without_renewing_it_and_said_to_renew() {
    let (found, asked) = found(&format!("echo '{ADA_RENEWED_BY_GH}'"), &token_is("ghu_made_up")).await;

    assert_eq!(found.unwrap(), ada(Some("ghu_made_up"), true));
    assert!(asked.lines().all(|run| run.ends_with("--no-refresh")), "asked {asked}");
}

/// Renewing is asking for the token without `--no-refresh`, as a run of `gh` the person makes would.
#[tokio::test]
async fn a_renewed_token_is_asked_for_without_no_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let path = gh(dir.path(), &format!("echo '{ADA_RENEWED_BY_GH}'"), &token_is("ghu_renewed"));

    let renewed = Gh::new(path, dir.path().to_path_buf()).renewed(GhTokens::TakenOut).await;

    assert_eq!(renewed, Some(GithubToken::new("ghu_renewed")));
    let asked = std::fs::read_to_string(dir.path().join("asked")).unwrap();
    assert_eq!(asked, "auth token --hostname github.com\n");
}

/// As a `gh` from 2.81 to before 2.103, which has no token it renews, says it.
#[tokio::test]
async fn a_gh_that_doesnt_know_no_refresh_is_asked_without_it() {
    let refuses = "case \"$*\" in *--no-refresh*) echo 'unknown flag: --no-refresh' >&2; exit 1;; esac";

    let (found, asked) = found(&format!("{refuses}\necho '{ADA}'"), &format!("{refuses}\necho gho_made_up")).await;

    assert_eq!(found.unwrap(), ada(Some("gho_made_up"), false));
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

    assert_eq!(found.unwrap(), GhRead::LoggedOut);
    assert!(!asked.contains("auth token"), "asked {asked}");
}

#[tokio::test]
async fn no_gh_on_the_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();

    let path = MachineEnvironment::only(&[("PATH", &dir.path().display().to_string())]);
    let found = Gh::new(path, dir.path().to_path_buf()).read(GhTokens::TakenOut).await;

    assert_eq!(found.unwrap(), GhRead::NotInstalled);
}

/// Something other than what `gh` prints, a `gh` older than 2.81, which doesn't know `--json` here, one that fails,
/// and a token from where it runs whose owner GitHub didn't say.
#[tokio::test]
async fn what_cant_be_read_is_refused_rather_than_taken_for_logged_out() {
    let printed = token_is("gho_made_up");
    let (garbage, _) = found("echo 'Logged in to github.com account ada (keyring)'", &printed).await;
    let (older, _) = found("echo 'unknown flag: --json' >&2\nexit 1", &printed).await;
    let (failed, _) = found(&format!("echo '{ADA}'\nexit 2"), &printed).await;
    let (nobody, _) = found(&format!("echo '{NOBODY_BY_TOKEN}'"), &printed).await;

    for found in [garbage, older, failed, nobody] {
        assert!(found.is_err(), "{found:?}");
    }
}

/// `gh`'s own login, and not a token the hub was started with or the profile exports, which `gh` would use over it,
/// and which `gh auth login` or `gh auth switch`, as a block says to run, wouldn't change. Read as agents have it, the
/// profile's token stays.
#[tokio::test]
async fn gh_is_read_without_the_tokens_the_hub_was_started_with() {
    let dir = tempfile::tempdir().unwrap();
    let machine = MachineEnvironment::only(&[("GH_TOKEN", "the-profiles"), ("PATH", "/bin")]);
    let gh = Gh::new(machine, dir.path().to_path_buf());
    let command = || {
        let mut command = std::process::Command::new("gh");
        command.env("GH_TOKEN", "the-hubs-own").env("GITHUB_TOKEN", "the-hubs-own").env("PATH", "/usr/bin");
        command
    };
    let (mut taken_out, mut as_agents) = (command(), command());

    gh.given(GhTokens::TakenOut).apply(&mut taken_out);
    gh.given(GhTokens::AsAgentsHaveThem).apply(&mut as_agents);

    let os = std::ffi::OsStr::new;
    let given: Vec<_> = taken_out.get_envs().collect();
    assert_eq!(given, [(os("GH_TOKEN"), None), (os("GITHUB_TOKEN"), None), (os("PATH"), Some(os("/bin")))]);
    let given: Vec<_> = as_agents.get_envs().collect();
    let expected = [("GH_TOKEN", "the-profiles"), ("GITHUB_TOKEN", "the-hubs-own"), ("PATH", "/bin")];
    assert_eq!(given, expected.map(|(name, value)| (os(name), Some(os(value)))));
}
