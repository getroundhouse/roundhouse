#![cfg(test)]

use super::examples;
use super::processes::Hub;
use client_core::{Client, Subscription, SubscriptionItem};
use examples::fake_agent;
use hub::Timers;
use local_hub::Daemon;
use process::testing::WholeEnvironment;
pub(crate) use process::testing::git;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Settings;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{AgentKind, Seq, Session, SessionId, SessionStatus, Workspace, WorkspaceId};
use protocol::paths::Paths;
use protocol::tail::Tail;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: Option<Hub>,
    /// The whole environment of every hub this starts, and so of its workers, and of anything else a test starts in
    /// the stack, such as a worker of its own. A test about one of the hub's time limits sets the hub's timers here,
    /// which go in place of [`Timers::FOR_TESTS`], and the workers' the same way.
    pub(crate) env: WholeEnvironment,
}

impl Stack {
    pub(crate) fn start() -> Stack {
        Stack::start_with(Vec::new())
    }

    /// With `env` set for every hub, as setting it in [`Stack::env`] does for the hubs started after.
    pub(crate) fn start_with(env: Vec<(&'static str, String)>) -> Stack {
        let mut stack = Stack::unstarted(env);
        // So its first hub opens a store that has run every migration, rather than running them all on nothing, which
        // only `Stack::start_from_nothing`'s tests are about. A hub started again opens what the one before left.
        hub::store::give_a_migrated_database(&stack.paths());
        stack.start_hub();
        stack
    }

    /// With its hub's data directory and database left for the hub to make, as a hub started by hand on a machine that
    /// has never run one makes them, for a test of what it makes.
    pub(crate) fn start_from_nothing() -> Stack {
        let mut stack = Stack::unstarted(Vec::new());
        stack.start_hub();
        stack
    }

    /// Its directory, its environment and its repository, with no hub, nor anything of one's, yet.
    fn unstarted(env: Vec<(&'static str, String)>) -> Stack {
        // Under /tmp because of the socket path limit, which the default temp directory can exceed.
        let dir = tempfile::Builder::new().prefix("ah-").tempdir_in("/tmp").unwrap();
        let mut hubs = hub_environment(dir.path());
        for (name, value) in env {
            hubs.set(name, value);
        }
        let stack = Stack { dir, hub: None, env: hubs };
        process::testing::repository_with_a_commit(&stack.repo());
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

    /// A hub from `exe`, in the stack's environment, logging where [`Stack::hub_log`] says.
    pub(crate) fn command_at(&self, exe: &Path) -> Command {
        let log = File::options().create(true).append(true).open(self.hub_log()).unwrap();
        let mut command = self.env.command(exe);
        command.arg("hub").stdout(Stdio::null()).stderr(log);
        command
    }

    /// The daemon at `path`, for a client to start a hub from, which runs in the stack's environment as the stack's
    /// own hubs do.
    pub(crate) fn daemon(&self, path: impl Into<PathBuf>) -> Daemon {
        Daemon::in_place(path).in_environment(self.env.vars())
    }

    /// Where every hub this starts writes its log. Beside its data rather than in it, since making the hub's directory
    /// first would leave it as open as this user's other files, where the hub keeps its own to this user alone.
    pub(crate) fn hub_log(&self) -> PathBuf {
        self.path("hub.log")
    }

    /// Starts a hub, and waits until it accepts connections. One that exits first fails the test at once, with what it
    /// wrote to its log, rather than once the wait has run out.
    pub(crate) fn start_hub(&mut self) {
        let (socket, log) = (self.socket(), self.hub_log());
        let logged_before = std::fs::metadata(&log).map_or(0, |log| log.len());
        let hub = self.hub.insert(self.command().spawn().unwrap().into());
        wait_until("the hub to accept connections", || {
            if let Some(status) = hub.try_wait().unwrap() {
                let logged = std::fs::read(&log).unwrap_or_default();
                let own = logged.get(usize::try_from(logged_before).unwrap()..).unwrap_or_default();
                panic!(
                    "the hub exited with {status} before it accepted a connection:\n{}",
                    String::from_utf8_lossy(own)
                );
            }
            Client::connect(&socket, "test").is_ok()
        });
    }

    pub(crate) fn client(&self) -> Client {
        Client::connect(&self.socket(), "test").unwrap()
    }

    pub(crate) fn hub_pid(&self) -> u32 {
        self.hub.as_ref().unwrap().id()
    }

    pub(crate) fn workspace(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with(client, Settings::default())
    }

    /// A workspace, with a chat in it that's planning, so its agent asks before a call that changes anything.
    pub(crate) fn workspace_planning(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with(client, in_mode(protocol::fixtures::plan_mode()))
    }

    pub(super) fn workspace_with(&self, client: &Client, options: Settings) -> (Workspace, Session) {
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
        let session = client.session_create(&workspace.id, AgentKind::Claude, options, None).unwrap();
        (workspace, session)
    }
}

/// What every hub of a stack in `dir` runs in: what [`WholeEnvironment::for_a_test`] gives, with a home of its own, so
/// a real ~/.conductor/settings.toml or `gh` login can't change what these tests do, a `gh` that's logged in to
/// nothing, the fake agent, and the timers that only make a test wait shortened.
fn hub_environment(dir: &Path) -> WholeEnvironment {
    let (timers, short) = Timers::FOR_TESTS.env();
    let (worker_timers, worker_short) = worker::Timers::FOR_TESTS.env();
    let mut environment = WholeEnvironment::for_a_test(&dir.join("user-home"));
    environment
        // Ahead of the person's own. A test that has `gh` answer, as `support::gh` does, puts its own ahead of this one.
        .put_first_on_path(process::testing::logged_out_gh())
        .set(protocol::paths::ENV_HOME, dir.join("home"))
        .set(protocol::paths::ENV_WORKSPACES, dir.join("workspaces"))
        .set("ROUNDHOUSE_CLAUDE_BIN", fake_agent())
        .set("FAKE_CLAUDE_STATE", dir.join("agent-state"))
        // So an agent started again doesn't sit through the CLI's 2 s before it asks after a task.
        .set("FAKE_CLAUDE_POLL_DEFAULT_MS", worker::Timers::FOR_TESTS.wait_poll.as_millis().to_string())
        // So a test waits out only the time limits it's about. The hub passes the workers' on to them.
        .set(timers, short)
        .set(worker_timers, worker_short);
    environment
}

/// A hub's timers for a test that a stop, an archive or a start that gives up doesn't wait for a worker to stop by
/// itself: [`Timers::FOR_TESTS`], with ten minutes for a worker to stop before the hub ends it as stuck. That's so much
/// longer than [`TIMEOUT`] that a hub that gets through one within a wait of the test's can't have sat through it,
/// however slow the machine, where a test against the product's 15 s by the clock fails a slow machine with no bug.
pub(crate) fn patient_with_workers() -> Timers {
    Timers { shutdown_grace: Duration::from_mins(10), ..Timers::FOR_TESTS }
}

/// Settings that set only the approval mode.
pub(crate) fn in_mode(mode: ApprovalMode) -> Settings {
    Settings { mode: Some(mode), ..Settings::default() }
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
    // Each wait ends once the condition holds, or once the progress has moved on, which starts the next.
    loop {
        let held = waiting::thread::until_ok(TIMEOUT, || {
            if condition() {
                return Ok(true);
            }
            let now = progress();
            if now == last {
                return Err(format!("timed out waiting for {what}"));
            }
            last = now;
            Ok(false)
        });
        if held {
            return;
        }
    }
}

pub(crate) fn read_all(client: &Client, session_id: &SessionId) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    let mut from = Seq::FIRST;
    loop {
        let (page, last) = client.session_read(session_id, from, 500, false).unwrap();
        let Some(end) = page.last().map(|e| e.seq) else { return events };
        events.extend(page);
        if end >= last {
            return events;
        }
        from = end.next();
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
        assert_eq!(event.seq.get(), i as u64 + 1, "gap or duplicate at index {i}");
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
                self.events.push(*event);
            }
            SubscriptionItem::Tail(tail) => self.tail = tail,
            SubscriptionItem::Delta(delta) => self.tail.apply(&delta),
            SubscriptionItem::Notice(_) | SubscriptionItem::Terminal(_) => {}
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
