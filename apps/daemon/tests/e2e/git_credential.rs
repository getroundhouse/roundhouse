//! What git uses for github.com in what a real worker starts, once its hub has set the git credential: real git,
//! asking the worker's own binary as its helper, in a workspace's setup script. And who the worker finds its machine's
//! own `gh` is logged in as, whatever credential it was set.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use protocol::frame;
use protocol::model::{GhLogin, GhLoginWorks, GithubToken};
use protocol::worker::{
    GitCredential, GitCredentialSet, GithubCredential, GithubLoginReply, Hello, SetupDone, SetupWorkspace,
    WORKER_PROTOCOL_VERSION, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};

use crate::support::install_executable;

/// Says it's started, then waits for `go` in the workspace, and asks git for a credential for a remote on github.com
/// whose URL names another user, and where a remote over SSH goes.
const ASKS_GIT: &str = r": > started
while [ ! -e go ]; do sleep 0.01; done
printf 'protocol=https\nhost=github.com\nusername=someone-else\n\n' | git credential fill > filled 2> said
git ls-remote --get-url git@github.com:ada/repo.git > url";

/// A worker serving its hub on its pipes, in a directory of its own whose `home` has a git config that stands in for
/// the machine's own login, and whose workspace's setup script runs [`ASKS_GIT`].
struct Worker {
    dir: tempfile::TempDir,
    child: Child,
    to: ChildStdin,
    from: BufReader<ChildStdout>,
}

impl Worker {
    fn start() -> Self {
        Self::start_with(Vec::new())
    }

    /// The same, with `env` set too.
    fn start_with(env: Vec<(&str, OsString)>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let machines_own = "!f() { echo username=machine; echo password=machine-token; }; f";
        std::fs::write(home.join(".gitconfig"), format!("[credential]\n\thelper = \"{machines_own}\"\n")).unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join(".conductor")).unwrap();
        std::fs::write(work.join(".conductor/settings.toml"), "[scripts]\nsetup = \"sh asks-git.sh\"\n").unwrap();
        std::fs::write(work.join("asks-git.sh"), ASKS_GIT).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_roundhoused"))
            .args(["worker", "--stdio", "--workspaces-root"])
            .arg(dir.path().join("workspaces"))
            .arg("--state-dir")
            .arg(dir.path().join("instance"))
            // None of this machine's own git config, which could hand over a real login.
            .env("HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GH_CONFIG_DIR")
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (to, from) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
        let mut worker = Self { dir, child, to, from };
        let hello = Hello {
            protocol_version: WORKER_PROTOCOL_VERSION,
            oldest_protocol_version: None,
            keep_running_without_hub: false,
        };
        worker.send(0, WorkerRequest::Hello(hello));
        worker.replies(&[0]);
        worker
    }

    fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    fn send(&mut self, id: u64, request: WorkerRequest) {
        self.to.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).unwrap();
    }

    /// The replies to `ids`, in whatever order they come.
    fn replies(&mut self, ids: &[u64]) -> BTreeMap<u64, WorkerOutcome> {
        self.replies_in_order(ids).into_iter().collect()
    }

    /// The replies to `ids`, in the order they came.
    fn replies_in_order(&mut self, ids: &[u64]) -> Vec<(u64, WorkerOutcome)> {
        let mut replies = Vec::new();
        while replies.len() < ids.len() {
            let mut line = String::new();
            assert_ne!(self.from.read_line(&mut line).unwrap(), 0, "the worker went before answering {ids:?}");
            if let WorkerFrame::Reply { id, outcome } = frame::decode(line.as_bytes()).unwrap()
                && ids.contains(&id)
            {
                replies.push((id, outcome));
            }
        }
        replies
    }

    fn setup(&self) -> WorkerRequest {
        let path = self.work().to_string_lossy().into_owned();
        WorkerRequest::SetupWorkspace(SetupWorkspace { repo_path: path.clone(), path, env: vec![] })
    }

    /// What the script's git said, once the worker has set each of `credentials` in turn: what it filled in, what it
    /// said, and where it sends a remote over SSH.
    fn git_after(mut self, credentials: Vec<GithubCredential>) -> (String, String, String) {
        let ids: Vec<u64> = (1..).take(credentials.len()).collect();
        for (&id, credential) in ids.iter().zip(credentials) {
            self.send(id, WorkerRequest::GitCredentialSet(GitCredentialSet { credential }));
        }
        assert!(self.replies(&ids).values().all(|answer| *answer == WorkerOutcome::Response(WorkerResponse::Ok)));
        std::fs::write(self.work().join("go"), "").unwrap();
        self.send(99, self.setup());
        let done = self.replies(&[99]).remove(&99).unwrap();
        assert_eq!(done, WorkerOutcome::Response(WorkerResponse::SetupDone(SetupDone { code: Some(0) })));
        self.git_said()
    }

    fn git_said(&self) -> (String, String, String) {
        let read = |file: &str| std::fs::read_to_string(self.work().join(file)).unwrap();
        (read("filled"), read("said"), read("url"))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn account(username: &str, token: &str) -> GithubCredential {
    GithubCredential::Account(GitCredential {
        username: username.into(),
        token: GithubToken::new(token),
        identity: None,
    })
}

fn blocked(reason: &str) -> GithubCredential {
    GithubCredential::Blocked { reason: reason.into() }
}

/// Its username, whatever the remote names, and none of the machine's helpers, which git would otherwise ask.
#[test]
fn git_in_what_the_worker_starts_gets_the_account_rather_than_the_machines_own() {
    let (filled, _, url) = Worker::start().git_after(vec![account("ada", "made-up")]);

    assert!(filled.contains("username=ada\npassword=made-up\n"), "{filled}");
    assert_eq!(url, "https://github.com/ada/repo.git\n", "a remote over SSH goes through the helper too");
}

#[test]
fn git_in_what_the_worker_starts_under_the_machines_own_gets_the_machines_own() {
    let (filled, _, url) = Worker::start().git_after(vec![account("ada", "made-up"), GithubCredential::MachineOwn]);

    assert!(filled.contains("username=machine\npassword=machine-token\n"), "{filled}");
    assert_eq!(url, "git@github.com:ada/repo.git\n");
}

#[test]
fn git_in_what_the_worker_starts_under_a_block_gets_nothing_and_says_why() {
    let (filled, said, _) = Worker::start().git_after(vec![blocked("Renew ada in Settings")]);

    assert_eq!(filled, "");
    assert!(said.contains("Renew ada in Settings"), "{said}");
}

/// Each set applies before the next frame is read, so they're answered in the order sent, and the last one sent is
/// what a process started before them all reads.
#[test]
fn sets_reach_git_in_a_process_started_before_them_in_the_order_sent() {
    let mut worker = Worker::start();
    worker.send(1, WorkerRequest::GitCredentialSet(GitCredentialSet { credential: account("ada", "first") }));
    worker.replies(&[1]);
    worker.send(2, worker.setup());
    wait_for(&worker.work().join("started"));
    let ids: Vec<u64> = (3..14).collect();
    let sets = ids.iter().map(|&id| match id % 3 {
        0 => blocked("Renew ada in Settings"),
        1 => account("ada", &format!("token-{id}")),
        _ => account("grace", &format!("token-{id}")),
    });
    for (&id, credential) in ids.iter().zip(sets) {
        worker.send(id, WorkerRequest::GitCredentialSet(GitCredentialSet { credential }));
    }

    let applied = worker.replies_in_order(&ids);
    std::fs::write(worker.work().join("go"), "").unwrap();
    worker.replies(&[2]);

    assert_eq!(applied.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ids);
    assert!(applied.iter().all(|(_, answer)| *answer == WorkerOutcome::Response(WorkerResponse::Ok)), "{applied:?}");
    let (filled, _, _) = worker.git_said();
    assert!(filled.contains("username=ada\npassword=token-13\n"), "{filled}");
}

/// A `gh` that says it's logged in to github.com by the token in its environment, as the one the worker's own
/// environment has, which it names as `the-machines-token`, and as `the-workers-credential` once it's given the
/// worker's files, which it reads over any token, and that GitHub names the same for that token.
const SAYS_WHOSE_LOGIN: &str = r#"#!/bin/sh
login=
[ "$GH_TOKEN" = machines-own ] && login=the-machines-token
[ -n "$GH_CONFIG_DIR$GIT_CONFIG_COUNT" ] && login=the-workers-credential
case "$1 $2" in
'auth status') printf '{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"%s","tokenSource":"GH_TOKEN"}]}}\n' "$login" ;;
'api --include') printf 'HTTP/2.0 200 OK\n\r\n{"login":"%s"}\n' "$login" ;;
*) exit 1 ;;
esac"#;

/// What the worker starts under the machine's own finds, so not the files of the account it was set, nor without the
/// `GH_TOKEN` its own environment has, which an account's credential takes out of what it starts.
#[test]
fn the_login_found_is_the_machines_own_while_an_accounts_credential_is_set() {
    let bin = tempfile::tempdir().unwrap();
    let source = bin.path().join("gh.source");
    std::fs::write(&source, SAYS_WHOSE_LOGIN).unwrap();
    install_executable(&source, &bin.path().join("gh"));
    let mut path = OsString::from(bin.path());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap());
    let mut worker = Worker::start_with(vec![("PATH", path), ("GH_TOKEN", "machines-own".into())]);

    worker.send(1, WorkerRequest::GitCredentialSet(GitCredentialSet { credential: account("ada", "made-up") }));
    worker.send(2, WorkerRequest::FindGithubLogin);
    let replies = worker.replies(&[1, 2]);

    assert_eq!(replies[&1], WorkerOutcome::Response(WorkerResponse::Ok));
    let login = GhLogin::LoggedIn { username: "the-machines-token".into(), works: GhLoginWorks::Yes };
    assert_eq!(replies[&2], WorkerOutcome::Response(WorkerResponse::GithubLogin(GithubLoginReply { login })));
}

fn wait_for(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.exists() {
        assert!(std::time::Instant::now() < deadline, "{} never came", path.display());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
