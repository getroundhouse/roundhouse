//! A worker running an agent's CLI under the login its hub sent, rather than the one on its machine.

use std::path::Path;
use std::time::Duration;

use protocol::frame;
use protocol::model::{AgentKind, AgentLogin, LoginToken};
use protocol::worker::{AccountFor, AgentSpec, TitleFor, WorkerCall, WorkerOutcome, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use super::{Worker, agent_start, cli, config, hello, reply, start_worker};

/// Appends what it was run for, the token it was given and the API key it was given, or `none`, to `ran`, in the
/// directory it's in. Asked who's logged in, it answers as the real CLI does under a token. As an agent it then waits,
/// and asked for a title it fails.
const SAYS_ITS_TOKEN: &str = r#"ran="$(dirname "$0")/ran"
case " $* " in
  " auth status --json ") what=account ;;
  *" --session-id "*) what=agent ;;
  *) what=title ;;
esac
printf '%s %s %s\n' "$what" "${CLAUDE_CODE_OAUTH_TOKEN:-none}" "${ANTHROPIC_API_KEY:-none}" >> "$ran"
case "$what" in
  account) echo '{"loggedIn":true,"authMethod":"oauth_token","apiProvider":"firstParty"}' ;;
  agent) exec sleep 100 ;;
  *) exit 1 ;;
esac"#;

fn login(token: &str) -> AgentLogin {
    AgentLogin { token: LoginToken::new(token) }
}

/// What the CLI in `dir` wrote of each time it ran, sorted, once it has run `times` times.
async fn ran(dir: &Path, times: usize) -> Vec<String> {
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
    .expect("the CLI to have run")
}

/// Each carries its own, so a worker never has to remember one between them. What a launch sets in the agent's
/// environment besides can't put another token in its place, or an API key before it.
#[tokio::test]
async fn an_agent_a_title_and_an_account_read_each_run_under_the_login_they_came_with() {
    let dir = tempfile::tempdir().unwrap();
    let Worker { read: hub_read, write: mut hub_write, running } =
        start_worker(config(dir.path(), Some(cli(dir.path(), SAYS_ITS_TOKEN))));
    let mut frames = BufReader::new(hub_read).lines();
    let env = [("CLAUDE_CODE_OAUTH_TOKEN", "from-the-env"), ("ANTHROPIC_API_KEY", "from-the-env")];
    let env = env.map(|(name, value)| (name.to_string(), value.to_string())).to_vec();
    let start = AgentSpec { env, login: Some(login("for-the-agent")), ..agent_start("s", dir.path(), None) };
    let account = AccountFor { agent: AgentKind::Claude, login: Some(login("for-the-account")) };
    let title = TitleFor {
        agent: AgentKind::Claude,
        prompt: "Fix it".into(),
        images: vec![],
        login: Some(login("for-the-title")),
    };
    let calls =
        [hello(), WorkerRequest::AgentStart(start), WorkerRequest::AccountFor(account), WorkerRequest::TitleFor(title)];

    let mut answered = Vec::new();
    for (id, request) in calls.into_iter().enumerate() {
        let id = id as u64;
        hub_write.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).await.unwrap();
        answered.push(reply(&mut frames, id).await);
    }

    assert_eq!(
        ran(dir.path(), 3).await,
        ["account for-the-account none", "agent for-the-agent none", "title for-the-title none"],
        "{answered:?}"
    );
    let WorkerOutcome::Response(WorkerResponse::Account(account)) = &answered[2] else { panic!("{answered:?}") };
    assert_eq!((account.logged_in, account.login.as_deref(), &account.id), (true, Some("Long-lived token"), &None));
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker didn't exit").unwrap();
}

/// So a worker on a machine with a login of its own goes on using it for a hub that holds none, and a call under a
/// login leaves nothing behind for the next.
#[tokio::test]
async fn without_a_login_the_cli_is_given_no_token_even_after_a_call_that_had_one() {
    let dir = tempfile::tempdir().unwrap();
    let Worker { read: hub_read, write: mut hub_write, running } =
        start_worker(config(dir.path(), Some(cli(dir.path(), SAYS_ITS_TOKEN))));
    let mut frames = BufReader::new(hub_read).lines();
    let under_a_login = AccountFor { agent: AgentKind::Claude, login: Some(login("for-the-first")) };
    let without = AccountFor { agent: AgentKind::Claude, login: None };
    let calls = [hello(), WorkerRequest::AccountFor(under_a_login), WorkerRequest::AccountFor(without)];

    for (id, request) in calls.into_iter().enumerate() {
        let id = id as u64;
        hub_write.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).await.unwrap();
        reply(&mut frames, id).await;
    }

    let own = |name: &str| std::env::var(name).unwrap_or_else(|_| "none".to_string());
    let without = format!("account {} {}", own("CLAUDE_CODE_OAUTH_TOKEN"), own("ANTHROPIC_API_KEY"));
    assert_eq!(ran(dir.path(), 2).await, ["account for-the-first none".to_string(), without]);
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker didn't exit").unwrap();
}
