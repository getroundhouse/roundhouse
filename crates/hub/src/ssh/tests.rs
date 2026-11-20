//! Reading whose key GitHub takes, against a stand-in for `ssh` on the `PATH`, which prints on stderr what GitHub
//! answers `ssh -T` with in each case, and exits as `ssh` does.

use std::path::Path;

use protocol::model::SshLogin;
use providers::local::MachinePath;

use super::Ssh;
use super::reading::said;

/// What GitHub greets a key it takes as `ada`'s with, after which `ssh` exits with 1, since GitHub gives no shell.
const GREETED: &str =
    "echo \"Hi ada! You've successfully authenticated, but GitHub does not provide shell access.\" >&2\nexit 1";

/// A stand-in for `ssh` in `dir`, alone on the `PATH` it's given, that writes what it's asked to `asked`, then runs
/// `answer`, which uses only what the shell has built in.
fn ssh(dir: &Path, answer: &str) -> MachinePath {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let asked = dir.join("asked").display().to_string();
    let script = format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > '{asked}'\n{answer}\n");
    std::fs::write(bin.join("ssh"), script).unwrap();
    std::fs::set_permissions(bin.join("ssh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    MachinePath::only(Some(bin.display().to_string()))
}

/// What [`Ssh::read`] makes of a stand-in that runs `answer`, and what the stand-in was asked.
async fn read(answer: &str) -> (SshLogin, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = ssh(dir.path(), answer);

    let read = Ssh::new(path).read().await;

    (read, std::fs::read_to_string(dir.path().join("asked")).unwrap_or_default())
}

#[tokio::test]
async fn a_key_github_takes_is_the_user_it_greets_never_asking_anything() {
    let (read, asked) = read(GREETED).await;

    assert_eq!(read, SshLogin::Key { username: "ada".into() });
    assert_eq!(asked, "-T -o BatchMode=yes -o ConnectTimeout=10 -- git@github.com\n");
}

#[tokio::test]
async fn a_machine_whose_keys_github_refuses_has_none() {
    let (read, _) = read("echo 'git@github.com: Permission denied (publickey).' >&2\nexit 255").await;

    assert_eq!(read, SshLogin::NoKey);
}

#[tokio::test]
async fn no_ssh_on_the_path_couldnt_tell() {
    let dir = tempfile::tempdir().unwrap();

    let read = Ssh::new(MachinePath::only(Some(dir.path().display().to_string()))).read().await;

    assert_eq!(read, SshLogin::CouldntTell);
}

/// A host key `ssh` doesn't know, which it won't ask about, a connection that failed, and a deploy key, which GitHub
/// greets by its repository.
#[test]
fn anything_else_couldnt_tell() {
    let host_key = "No ED25519 host key is known for github.com and you have requested strict checking.\nHost key \
                    verification failed.";
    let unreached = "ssh: connect to host github.com port 22: Operation timed out";
    let deploy_key = "Hi ada/widgets! You've successfully authenticated, but GitHub does not provide shell access.";

    for stderr in [host_key, unreached, deploy_key, ""] {
        assert_eq!(said(stderr), SshLogin::CouldntTell, "for {stderr}");
    }
}

/// What a warning `ssh` prints first, such as of a host key it has just learned, doesn't hide the greeting after it.
#[test]
fn the_greeting_is_found_after_what_ssh_prints_first() {
    let stderr = "Warning: Permanently added 'github.com' (ED25519) to the list of known hosts.\nHi ada! You've \
                  successfully authenticated, but GitHub does not provide shell access.";

    assert_eq!(said(stderr), SshLogin::Key { username: "ada".into() });
}
