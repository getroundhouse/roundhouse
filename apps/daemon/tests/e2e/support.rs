//! What the tests share: a running stack, and ways to wait on and read what it does.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use client_core::{Client, Subscription, SubscriptionItem};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{
    AgentKind, ApprovalId, PermissionMode, Session, SessionId, SessionSettingsChange, SessionStatus, Workspace,
    WorkspaceId,
};
use protocol::paths::Paths;
use protocol::tail::Tail;
use tempfile::TempDir;

use examples::fake_agent;
pub(crate) use feed::next_notice;
pub(crate) use processes::{is_alive, parent_of, signal, terminate};

mod examples;
mod feed;
pub(crate) mod gh;
mod processes;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

/// Where `gh` finds a login other than in `HOME`.
const GH_LOGINS: [&str; 6] =
    ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN", "GH_CONFIG_DIR", "XDG_CONFIG_HOME"];

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: Option<Child>,
    /// Set for every hub this starts, and so for its workers.
    env: Vec<(&'static str, String)>,
}

impl Stack {
    pub(crate) fn start() -> Stack {
        Stack::start_with(Vec::new())
    }

    pub(crate) fn start_with(env: Vec<(&'static str, String)>) -> Stack {
        // Under /tmp because of the socket path limit, which the default temp directory can exceed.
        let dir = tempfile::Builder::new().prefix("ah-").tempdir_in("/tmp").unwrap();
        let mut stack = Stack { dir, hub: None, env };
        std::fs::create_dir_all(stack.repo()).unwrap();
        git(&stack.repo(), &["init", "-q", "-b", "main"]);
        std::fs::write(stack.repo().join("a.txt"), "one\n").unwrap();
        git(&stack.repo(), &["add", "."]);
        git(&stack.repo(), &["commit", "-qm", "first"]);
        stack.start_hub();
        stack
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.path("repo")
    }

    pub(crate) fn socket(&self) -> PathBuf {
        self.path("home/hub.sock")
    }

    /// Where a client looks for this stack's hub, and starts one if there's none.
    pub(crate) fn paths(&self) -> Paths {
        protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME => Some(self.path("home").to_str().unwrap().to_string()),
            protocol::paths::ENV_WORKSPACES => Some(self.path("workspaces").to_str().unwrap().to_string()),
            _ => None,
        })
        .unwrap()
    }

    pub(crate) fn agent_state(&self) -> PathBuf {
        self.path("agent-state")
    }

    pub(crate) fn command(&self) -> Command {
        self.command_at(Path::new(env!("CARGO_BIN_EXE_roundhoused")))
    }

    pub(crate) fn command_at(&self, exe: &Path) -> Command {
        let mut command = Command::new(exe);
        command
            .arg("hub")
            .env("ROUNDHOUSE_HOME", self.path("home"))
            .env("ROUNDHOUSE_WORKSPACES", self.path("workspaces"))
            .env("ROUNDHOUSE_CLAUDE_BIN", fake_agent())
            .env("FAKE_CLAUDE_STATE", self.agent_state())
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            .env("HOME", self.path("user-home"))
            .envs(self.env.iter().map(|(name, value)| (name, value)))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // So `gh` can't reach GitHub as whoever runs the tests, any more than it finds their login in `HOME`.
        for name in GH_LOGINS {
            command.env_remove(name);
        }
        command
    }

    /// Set for every hub started from now on, as [`Stack::start_with`] sets it for the first.
    pub(crate) fn add_env(&mut self, name: &'static str, value: String) {
        self.env.push((name, value));
    }

    pub(crate) fn start_hub(&mut self) {
        self.hub = Some(self.command().spawn().unwrap());
        wait_until("the hub to accept connections", || Client::connect(&self.socket(), "test").is_ok());
    }

    pub(crate) fn client(&self) -> Client {
        Client::connect(&self.socket(), "test").unwrap()
    }

    pub(crate) fn hub_pid(&self) -> u32 {
        self.hub.as_ref().unwrap().id()
    }

    pub(crate) fn workspace(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with(client, SessionSettingsChange::default())
    }

    /// A workspace, with a chat in it that's planning, so its agent asks before a call that changes anything.
    pub(crate) fn workspace_planning(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with(client, in_mode(PermissionMode::Plan))
    }

    fn workspace_with(&self, client: &Client, options: SessionSettingsChange) -> (Workspace, Session) {
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
        let session = client.session_create(&workspace.id, AgentKind::Claude, options, None).unwrap();
        (workspace, session)
    }
}

/// Settings that set only the approval mode.
pub(crate) fn in_mode(mode: PermissionMode) -> SessionSettingsChange {
    SessionSettingsChange { permission_mode: Some(mode), ..SessionSettingsChange::default() }
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) fn wait_until(what: &str, condition: impl FnMut() -> bool) {
    wait_while_progressing(what, || (), condition);
}

/// Waits for `condition` for as long as `progress` keeps changing, and gives up once it has stayed the
/// same for `TIMEOUT`. For work long enough that a busy machine can take more than `TIMEOUT` over it, so
/// the wait fails when the work stops rather than when it's slow.
pub(crate) fn wait_while_progressing<P: PartialEq>(
    what: &str,
    mut progress: impl FnMut() -> P,
    mut condition: impl FnMut() -> bool,
) {
    let mut last = progress();
    let mut deadline = Instant::now() + TIMEOUT;
    while !condition() {
        let now = progress();
        if now != last {
            last = now;
            deadline = Instant::now() + TIMEOUT;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn read_all(client: &Client, session_id: &SessionId) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    let mut from = 1;
    loop {
        let (page, last) = client.session_read(session_id, from, 500, false).unwrap();
        let Some(end) = page.last().map(|e| e.seq) else { return events };
        events.extend(page);
        if end >= last {
            return events;
        }
        from = end + 1;
    }
}

pub(crate) fn wait_for_status(client: &Client, session_id: &SessionId, status: SessionStatus) {
    wait_until(&format!("session to be {status:?}"), || client.session_get(session_id).unwrap().status == status);
}

/// Waits until the agent has finished the turn, and everything it said is in the log.
pub(crate) fn wait_for_turn_end(client: &Client, session_id: &SessionId, count: usize) -> Vec<SessionEvent> {
    wait_for_turn_end_while_progressing(client, session_id, count, || ())
}

/// Like `wait_for_turn_end`, but giving up only once `progress` stops changing, as `wait_while_progressing`
/// does.
pub(crate) fn wait_for_turn_end_while_progressing<P: PartialEq>(
    client: &Client,
    session_id: &SessionId,
    count: usize,
    progress: impl FnMut() -> P,
) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    wait_while_progressing(&format!("{count} turn(s) to end"), progress, || {
        events = read_all(client, session_id);
        let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
        ended >= count && client.session_get(session_id).unwrap().status == SessionStatus::Idle
    });
    events
}

pub(crate) fn started_pid(events: &[SessionEvent]) -> Option<u32> {
    events.iter().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    })
}

pub(crate) fn latest_started_pid(events: &[SessionEvent]) -> Option<u32> {
    events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    })
}

pub(crate) fn assert_gapless(events: &[SessionEvent]) {
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, i as u64 + 1, "gap or duplicate at index {i}");
    }
}

pub(crate) fn assistant_text(events: &[SessionEvent]) -> String {
    let mut text = String::new();
    for event in events {
        if let EventBody::Assistant { blocks, .. } = &event.body {
            for block in blocks {
                if let ContentBlock::Text { text: t, .. } = block {
                    text.push_str(t);
                }
            }
        }
    }
    text
}

pub(crate) fn pending_approval(events: &[SessionEvent]) -> Option<ApprovalId> {
    let mut open = None;
    for event in events {
        match &event.body {
            EventBody::ApprovalRequested { approval_id, .. } => open = Some(approval_id.clone()),
            EventBody::ApprovalResolved { approval_id, .. } if open.as_ref() == Some(approval_id) => open = None,
            _ => {}
        }
    }
    open
}

pub(crate) fn wait_for_approval(client: &Client, session_id: &SessionId) -> ApprovalId {
    let mut found = None;
    wait_until("an approval request", || {
        found = pending_approval(&read_all(client, session_id));
        found.is_some()
    });
    found.unwrap()
}

/// What a client would hold after reading a subscription: its events, and its streaming tail.
#[derive(Default)]
pub(crate) struct Follower {
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) tail: Tail,
}

impl Follower {
    pub(crate) fn apply(&mut self, item: SubscriptionItem) {
        match item {
            SubscriptionItem::Event(event) => {
                self.tail.on_event(event.seq, event.run, &event.body);
                self.events.push(event);
            }
            SubscriptionItem::Tail(tail) => self.tail = tail,
            SubscriptionItem::Delta(delta) => self.tail.apply(&delta),
            SubscriptionItem::Notice(_) => {}
        }
    }

    pub(crate) fn drain_until(
        &mut self,
        subscription: &Subscription,
        what: &str,
        mut done: impl FnMut(&Follower) -> bool,
    ) {
        let deadline = Instant::now() + TIMEOUT;
        while !done(self) {
            let wait = deadline.saturating_duration_since(Instant::now());
            match subscription.items.recv_timeout(wait) {
                Ok(item) => self.apply(item),
                Err(e) => panic!("timed out waiting for {what}: {e}"),
            }
        }
    }

    pub(crate) fn tail_text(&self) -> String {
        self.tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }
}

/// Commits Conductor-style settings to the test repository.
pub(crate) fn commit_settings(stack: &Stack, toml: &str) {
    let dir = stack.repo().join(".conductor");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.toml"), toml).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "settings"]);
}

/// The workspace as the hub has it now, archived or not.
pub(crate) fn workspace(client: &Client, id: &WorkspaceId) -> protocol::model::Workspace {
    client.workspace_list(true).unwrap().into_iter().find(|w| &w.id == id).unwrap()
}

pub(crate) fn wait_for_setup(client: &Client, id: &WorkspaceId) -> protocol::model::SetupStatus {
    let mut status = protocol::model::SetupStatus::Running;
    wait_until("setup to finish", || {
        status = workspace(client, id).setup;
        !matches!(status, protocol::model::SetupStatus::Running | protocol::model::SetupStatus::NotStarted)
    });
    status
}

/// Through `install`, because writing an executable from this multithreaded process can break running
/// it on Linux. A fork in another test briefly inherits the open file, and `exec` then fails with
/// "text file busy". This way, this process never has the file open for writing.
pub(crate) fn install_executable(source: &Path, target: &Path) {
    let _ = std::fs::remove_file(target);
    let status = Command::new("install").args(["-m", "755"]).arg(source).arg(target).status().unwrap();
    assert!(status.success());
}
