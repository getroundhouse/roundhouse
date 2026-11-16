//! A worker handing the git credential its hub set to what it starts, and leaving no token behind.

use std::path::Path;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::frame;
use protocol::model::{AgentKind, GithubToken};
use protocol::worker::{
    AccountFor, GitCredential, GitCredentialSet, GithubCredential, TitleFor, WorkerCall, WorkerFrame, WorkerOutcome,
    WorkerRequest, WorkerResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use super::{Worker, agent_start, cli, config, hello, next_frame, reply, send, start_worker};

fn set(credential: GithubCredential) -> WorkerRequest {
    WorkerRequest::GitCredentialSet(GitCredentialSet { credential })
}

fn ada() -> GithubCredential {
    GithubCredential::Account(GitCredential { username: "ada".into(), token: GithubToken::new("made-up") })
}

/// The directory a worker in `dir` keeps the files in.
fn files(dir: &Path) -> std::path::PathBuf {
    dir.join("state").join("git-credential")
}

/// What the CLI in `dir` wrote of each time it started, sorted, once it has started `times` times.
async fn started(dir: &Path, times: usize) -> Vec<String> {
    let ran = dir.join("ran");
    timeout(Duration::from_secs(10), async {
        loop {
            let mut lines: Vec<String> =
                std::fs::read_to_string(&ran).unwrap_or_default().lines().map(str::to_string).collect();
            if lines.len() >= times {
                lines.sort();
                return lines;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the CLI to have started {times} times: {:?}", std::fs::read_to_string(&ran)))
}

/// A one-off call as well as an agent, since each starts the CLI its own way. One started under the machine's own
/// is given nothing, so it keeps the machine's own.
#[tokio::test]
async fn an_agent_and_a_one_off_call_are_given_the_files_only_while_the_credential_isnt_the_machines_own() {
    let dir = tempfile::tempdir().unwrap();
    // Its first two words, which tell an agent from a title and an account read, and where it's told git's config is.
    // In `dir`, since a title is asked in a directory of its own.
    let ran = dir.path().join("ran").display().to_string();
    let records = format!(
        r#"printf '%s %s %s\n' "$1" "$2" "${{GIT_CONFIG_VALUE_0:-none}}" >> '{ran}'
[ "$2" = --input-format ] && exec sleep 100"#
    );
    let Worker { read: hub_read, write: mut hub_write, running } =
        start_worker(config(dir.path(), Some(cli(dir.path(), &records))));
    let mut frames = BufReader::new(hub_read).lines();
    let title = TitleFor { agent: AgentKind::Claude, prompt: "Fix it".into(), images: vec![], login: None };
    let calls = [
        hello(),
        set(ada()),
        WorkerRequest::AgentStart(agent_start("s1", dir.path(), None)),
        WorkerRequest::TitleFor(title),
        WorkerRequest::AccountFor(AccountFor { agent: AgentKind::Claude, login: None }),
    ];

    send(&mut hub_write, calls).await;
    // In whatever order they finish, since each but the set runs on a task of its own.
    let mut answered = std::collections::BTreeSet::new();
    while answered.len() < 4 {
        if let WorkerFrame::Reply { id, .. } = next_frame(&mut frames).await {
            answered.insert(id);
        }
    }
    let under_an_account = started(dir.path(), 3).await;
    // After the agent above has started, which it has once its start is answered.
    for (id, request) in
        (5..).zip([set(GithubCredential::MachineOwn), WorkerRequest::AgentStart(agent_start("s2", dir.path(), None))])
    {
        hub_write.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).await.unwrap();
    }
    reply(&mut frames, 6).await;
    let all = started(dir.path(), 4).await;

    let config = files(dir.path()).join("config").display().to_string();
    let mut expected = ["-p --input-format", "-p --model", "auth status"].map(|words| format!("{words} {config}"));
    expected.sort();
    assert_eq!(under_an_account, expected);
    // MachineOwn inherits the caller's environment, which can itself come from a Roundhouse worker.
    let inherited = std::env::var("GIT_CONFIG_VALUE_0").ok().filter(|value| !value.is_empty());
    let mut expected = expected.to_vec();
    expected.push(format!("-p --input-format {}", inherited.as_deref().unwrap_or("none")));
    expected.sort();
    assert_eq!(all, expected);
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker to exit").unwrap();
}

/// So no snapshot of the machine keeps a token, and a worker that was killed leaves none for long.
#[tokio::test]
async fn a_worker_removes_what_the_last_left_as_it_starts_and_its_own_as_it_exits() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(files(dir.path())).unwrap();
    std::fs::write(files(dir.path()).join("credential.json"), r#"{"kind":"account","username":"x","token":"x"}"#)
        .unwrap();
    let Worker { read: hub_read, write: mut hub_write, running } = start_worker(config(dir.path(), None));
    let mut frames = BufReader::new(hub_read).lines();

    send(&mut hub_write, [hello(), set(ada())]).await;
    let greeted = reply(&mut frames, 0).await;
    let left_by_the_last = files(dir.path()).exists();
    let applied = reply(&mut frames, 1).await;
    let its_own = files(dir.path()).join("credential.json").exists();
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker to exit").unwrap();

    assert!(matches!(greeted, WorkerOutcome::Response(WorkerResponse::Hello(_))), "{greeted:?}");
    assert!(!left_by_the_last, "what the last worker left was still there once this one had said hello");
    assert_eq!(applied, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(its_own);
    assert!(!files(dir.path()).exists(), "this worker's files were still there once it had exited");
}

/// One that another worker already serves the instance for, whose files are that worker's.
#[tokio::test]
async fn a_worker_refused_the_instance_leaves_the_files_of_the_one_serving_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(files(dir.path())).unwrap();
    std::fs::write(files(dir.path()).join("credential.json"), "{}").unwrap();
    let refused = worker::Config { refused: Some("another worker is serving it".into()), ..config(dir.path(), None) };
    let Worker { read: hub_read, write: mut hub_write, running } = start_worker(refused);
    let mut frames = BufReader::new(hub_read).lines();

    send(&mut hub_write, [hello(), set(GithubCredential::MachineOwn)]).await;
    let answered = reply(&mut frames, 1).await;
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker to exit").unwrap();

    assert!(matches!(&answered, WorkerOutcome::Error(e) if e.code == ErrorCode::Conflict), "{answered:?}");
    assert!(files(dir.path()).join("credential.json").exists());
}
