//! What the tests share: a running stack, and ways to wait on and read what it does.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use client_core::{Client, Subscription, SubscriptionItem};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{AgentKind, ApprovalId, Session, SessionId, SessionStatus, Workspace, WorkspaceId};
use protocol::tail::Tail;
use tempfile::TempDir;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

/// Returns what `kill` did. Signal 0 only checks that the process exists.
pub(crate) fn signal(pid: u32, signal: i32) -> i32 {
    // SAFETY: `kill` takes any PID and signal, and touches no memory of ours.
    unsafe { libc::kill(i32::try_from(pid).unwrap(), signal) }
}

/// `cargo test --test e2e` doesn't build examples, so a plain path could run an out-of-date fake
/// without saying so. Building here means tests always use the current one.
pub(crate) fn fake_agent() -> &'static Path {
    static FAKE: OnceLock<PathBuf> = OnceLock::new();
    FAKE.get_or_init(|| {
        let bin_dir = Path::new(env!("CARGO_BIN_EXE_roundhoused")).parent().unwrap();
        let mut build = Command::new(env!("CARGO"));
        build.args(["build", "-p", "daemon", "--example", "fake_claude"]);
        if bin_dir.file_name().is_some_and(|name| name == "release") {
            build.arg("--release");
        }
        let output = build.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        bin_dir.join("examples/fake_claude")
    })
}

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: Option<Child>,
}

impl Stack {
    pub(crate) fn start() -> Stack {
        // Under /tmp because of the socket path limit, which the default temp directory can exceed.
        let dir = tempfile::Builder::new().prefix("ah-").tempdir_in("/tmp").unwrap();
        let mut stack = Stack { dir, hub: None };
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
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
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

    pub(crate) fn kill_hub(&mut self) {
        let mut hub = self.hub.take().unwrap();
        hub.kill().unwrap();
        hub.wait().unwrap();
    }

    pub(crate) fn workspace(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with_mode(client, Some("bypassPermissions"))
    }

    pub(crate) fn workspace_with_mode(&self, client: &Client, mode: Option<&str>) -> (Workspace, Session) {
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
        let session =
            client.session_create(&workspace.id, AgentKind::Claude, None, mode.map(String::from), None).unwrap();
        (workspace, session)
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Some(mut hub) = self.hub.take() {
            // SIGTERM first, so the worker and agent get to stop rather than being left over.
            terminate(hub.id());
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if !matches!(hub.try_wait(), Ok(None)) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = hub.kill();
            let _ = hub.wait();
        }
    }
}

pub(crate) fn terminate(pid: u32) {
    signal(pid, libc::SIGTERM);
}

pub(crate) fn is_alive(pid: u32) -> bool {
    // A zombie still counts as existing, which is why
    // tests also look at its state below.
    if signal(pid, 0) != 0 {
        return false;
    }
    let state = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
    let state = String::from_utf8_lossy(&state.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
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

pub(crate) fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
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
    let mut events = Vec::new();
    wait_until(&format!("{count} turn(s) to end"), || {
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

pub(crate) fn parent_of(pid: u32) -> u32 {
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
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
                if let ContentBlock::Text { text: t } = block {
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
                self.tail.on_event(event.run, &event.body);
                self.events.push(event);
            }
            SubscriptionItem::Tail(tail) => self.tail = tail,
            SubscriptionItem::Delta(delta) => self.tail.apply(&delta),
            SubscriptionItem::Notice(_) | SubscriptionItem::Disconnected => {}
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

pub(crate) fn wait_for_setup(client: &Client, id: &WorkspaceId) -> protocol::model::SetupStatus {
    let mut status = protocol::model::SetupStatus::Running;
    wait_until("setup to finish", || {
        status = client.workspace_list(true).unwrap().into_iter().find(|w| &w.id == id).unwrap().setup;
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
