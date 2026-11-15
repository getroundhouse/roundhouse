use std::path::Path;
use std::process::Command;

use protocol::worker::GithubCredential;

use super::super::files::{Files, Handed};
use super::{ada, files};

fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// What git is given for each account and block, whose helper is the one [`files`] names.
fn config(dir: &Path) -> String {
    // As a git config file quotes it, which on Windows doubles each of the path's backslashes.
    let dir = dir.display().to_string().replace('\\', r"\\");
    let helper = format!("!'/opt/Round House/roundhoused' 'git-credential' '--' '{dir}'");
    format!(
        "# Rewritten by the Roundhouse worker whenever its hub sets the git credential.\n\
         [credential \"https://github.com\"]\n\thelper =\n\thelper = \"{helper}\"\n\
         [url \"https://github.com/\"]\n\tinsteadOf = git@github.com:\n\tinsteadOf = ssh://git@github.com/\n"
    )
}

#[test]
fn an_account_sends_github_to_the_helper_and_logs_gh_in_as_it() {
    let state = tempfile::tempdir().unwrap();
    let dir = state.path().join("git-credential");

    files(state.path()).write(&Handed::Account(ada())).unwrap();

    assert_eq!(read(dir.join("config")), config(&dir));
    let answer: GithubCredential = serde_json::from_str(&read(dir.join("credential.json"))).unwrap();
    assert_eq!(answer, GithubCredential::Account(ada()));
    assert_eq!(
        read(dir.join("gh/hosts.yml")),
        "github.com:\n    users:\n        \"ada\":\n            oauth_token: \"made-up\"\n    git_protocol: https\n    \
         oauth_token: \"made-up\"\n    user: \"ada\"\n"
    );
}

/// After an account, as when its token ends under an agent that's running.
#[test]
fn a_block_keeps_the_helper_and_logs_gh_in_with_a_token_github_refuses() {
    let state = tempfile::tempdir().unwrap();
    let dir = state.path().join("git-credential");
    let files = files(state.path());
    files.write(&Handed::Account(ada())).unwrap();

    files.write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();

    assert_eq!(read(dir.join("config")), config(&dir));
    let answer: GithubCredential = serde_json::from_str(&read(dir.join("credential.json"))).unwrap();
    assert_eq!(answer, GithubCredential::Blocked { reason: "Renew ada in Settings".into() });
    let hosts = read(dir.join("gh/hosts.yml"));
    assert!(hosts.contains("oauth_token: \"blocked-by-roundhouse\""), "{hosts}");
    assert!(!hosts.contains("\"made-up\""), "the account's token is gone: {hosts}");
}

#[test]
fn removing_takes_the_directory_and_everything_in_it() {
    let state = tempfile::tempdir().unwrap();
    let files = files(state.path());
    files.write(&Handed::Account(ada())).unwrap();

    files.remove().unwrap();

    assert!(!state.path().join("git-credential").exists());
    files.remove().unwrap();
}

/// Git reads the helper back as the shell is to run it, whatever the worker's paths hold.
#[test]
fn git_reads_the_helper_as_it_was_written_whatever_its_path_holds() {
    let state = tempfile::tempdir().unwrap();
    // A quote and a backslash only where a file's name can have them, and Windows' paths have backslashes anyway.
    let odd = state.path().join(if cfg!(windows) { "it's odd #1; $HOME" } else { r#"it's "odd" \ #1; $HOME"# });
    let files = Files::new(&odd, vec!["/opt/roundhoused".into()]);
    files.write(&Handed::Account(ada())).unwrap();
    let config = odd.join("git-credential").join("config");

    let read = Command::new("git")
        .args(["config", "--file"])
        .arg(&config)
        .args(["--get-all", "credential.https://github.com.helper"])
        .output()
        .unwrap();

    let dir = odd.join("git-credential").display().to_string().replace('\'', r"'\''");
    assert_eq!(String::from_utf8(read.stdout).unwrap(), format!("\n!'/opt/roundhoused' '{dir}'\n"));
}

/// Without a helper, since only where a remote goes is asked.
#[test]
fn a_process_given_the_files_sends_githubs_ssh_addresses_over_https() {
    let state = tempfile::tempdir().unwrap();
    let files = files(state.path());
    files.write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();
    let url = |remote: &str| {
        let mut git = Command::new("git");
        // Without the person's own config, which can send GitHub over SSH, as "What still goes round it" says.
        git.args(["ls-remote", "--get-url", remote]).env("HOME", state.path()).env("GIT_CONFIG_NOSYSTEM", "1");
        git.env_remove("XDG_CONFIG_HOME");
        files.environment().apply(&mut git);
        String::from_utf8(git.output().unwrap().stdout).unwrap()
    };

    assert_eq!(url("git@github.com:ada/repo.git"), "https://github.com/ada/repo.git\n");
    assert_eq!(url("ssh://git@github.com/ada/repo.git"), "https://github.com/ada/repo.git\n");
    assert_eq!(url("git@gitlab.com:ada/repo.git"), "git@gitlab.com:ada/repo.git\n", "only GitHub's");
}

#[test]
fn a_process_given_the_files_reads_them_rather_than_any_token_it_would_inherit() {
    let state = tempfile::tempdir().unwrap();
    let files = files(state.path());
    let mut command = Command::new("gh");
    command.env("GH_TOKEN", "the-machines-own").env("GIT_CONFIG_COUNT", "2");

    files.environment().apply(&mut command);

    let given: Vec<_> = command.get_envs().map(|(name, value)| (name.to_str().unwrap(), value)).collect();
    let dir = state.path().join("git-credential");
    for taken_out in ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"] {
        assert!(given.contains(&(taken_out, None)), "{taken_out}: {given:?}");
    }
    assert!(given.contains(&("GIT_CONFIG_COUNT", Some("1".as_ref()))), "{given:?}");
    assert!(given.contains(&("GIT_CONFIG_KEY_0", Some("include.path".as_ref()))), "{given:?}");
    assert!(given.contains(&("GIT_CONFIG_VALUE_0", Some(dir.join("config").as_os_str()))), "{given:?}");
    assert!(given.contains(&("GH_CONFIG_DIR", Some(dir.join("gh").as_os_str()))), "{given:?}");
}

/// The modes, which only Unix has.
#[cfg(unix)]
#[test]
fn only_this_user_can_open_them() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let state = tempfile::tempdir().unwrap();
    let dir = state.path().join("git-credential");

    files(state.path()).write(&Handed::Account(ada())).unwrap();

    assert_eq!((mode(&dir), mode(&dir.join("gh"))), (0o700, 0o700));
    for file in ["config", "credential.json", "gh/hosts.yml"] {
        assert_eq!(mode(&dir.join(file)), 0o600, "{file}");
    }
}
