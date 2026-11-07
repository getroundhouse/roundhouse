//! The whole stack with real processes: a real hub binary, real workers, real git worktrees and a
//! real socket. Only the agent is fake. See `examples/fake_claude.rs`.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use client_core::{Client, ClientError, Subscription, SubscriptionItem};
use protocol::client::{ClientRequest, ErrorCode, HubNotice};
use protocol::event::{
    ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody, ExitReason, SessionEvent, TurnOutcome,
};
use protocol::model::{AgentKind, DiffSide, Session, SessionStatus, Workspace, WorkspaceStatus};
use protocol::tail::Tail;
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(20);

/// `cargo test --test e2e` doesn't build examples, so a plain path could run an out-of-date fake
/// without saying so. Building here means tests always use the current one.
/// Returns what `kill` did. Signal 0 only checks that the process exists.
fn signal(pid: u32, signal: i32) -> i32 {
    // SAFETY: `kill` takes any PID and signal, and touches no memory of ours.
    unsafe { libc::kill(i32::try_from(pid).unwrap(), signal) }
}

fn fake_agent() -> &'static Path {
    static FAKE: OnceLock<PathBuf> = OnceLock::new();
    FAKE.get_or_init(|| {
        let bin_dir = Path::new(env!("CARGO_BIN_EXE_trestled")).parent().unwrap();
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

struct Stack {
    dir: TempDir,
    hub: Option<Child>,
}

impl Stack {
    fn start() -> Stack {
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

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn repo(&self) -> PathBuf {
        self.path("repo")
    }

    fn socket(&self) -> PathBuf {
        self.path("home/hub.sock")
    }

    fn agent_state(&self) -> PathBuf {
        self.path("agent-state")
    }

    fn command(&self) -> Command {
        self.command_at(Path::new(env!("CARGO_BIN_EXE_trestled")))
    }

    fn command_at(&self, exe: &Path) -> Command {
        let mut command = Command::new(exe);
        command
            .arg("hub")
            .env("TRESTLE_HOME", self.path("home"))
            .env("TRESTLE_WORKSPACES", self.path("workspaces"))
            .env("TRESTLE_CLAUDE_BIN", fake_agent())
            .env("FAKE_CLAUDE_STATE", self.agent_state())
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            .env("HOME", self.path("user-home"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn start_hub(&mut self) {
        self.hub = Some(self.command().spawn().unwrap());
        wait_until("the hub to accept connections", || Client::connect(&self.socket(), "test").is_ok());
    }

    fn client(&self) -> Client {
        Client::connect(&self.socket(), "test").unwrap()
    }

    fn hub_pid(&self) -> u32 {
        self.hub.as_ref().unwrap().id()
    }

    fn kill_hub(&mut self) {
        let mut hub = self.hub.take().unwrap();
        hub.kill().unwrap();
        hub.wait().unwrap();
    }

    fn workspace(&self, client: &Client) -> (Workspace, Session) {
        self.workspace_with_mode(client, Some("bypassPermissions"))
    }

    fn workspace_with_mode(&self, client: &Client, mode: Option<&str>) -> (Workspace, Session) {
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None).unwrap();
        let session = client.session_create(&workspace.id, AgentKind::Claude, None, mode.map(String::from)).unwrap();
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

fn terminate(pid: u32) {
    signal(pid, libc::SIGTERM);
}

fn is_alive(pid: u32) -> bool {
    // A zombie still counts as existing, which is why
    // tests also look at its state below.
    if signal(pid, 0) != 0 {
        return false;
    }
    let state = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
    let state = String::from_utf8_lossy(&state.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}

fn git(cwd: &Path, args: &[&str]) -> String {
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

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn read_all(client: &Client, session_id: &str) -> Vec<SessionEvent> {
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

fn wait_for_status(client: &Client, session_id: &str, status: SessionStatus) {
    wait_until(&format!("session to be {status:?}"), || client.session_get(session_id).unwrap().status == status);
}

/// Waits until the agent has finished the turn, and everything it said is in the log.
fn wait_for_turn_end(client: &Client, session_id: &str, count: usize) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    wait_until(&format!("{count} turn(s) to end"), || {
        events = read_all(client, session_id);
        let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
        ended >= count && client.session_get(session_id).unwrap().status == SessionStatus::Idle
    });
    events
}

fn started_pid(events: &[SessionEvent]) -> Option<u32> {
    events.iter().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    })
}

fn latest_started_pid(events: &[SessionEvent]) -> Option<u32> {
    events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    })
}

fn parent_of(pid: u32) -> u32 {
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

fn assert_gapless(events: &[SessionEvent]) {
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, i as u64 + 1, "gap or duplicate at index {i}");
    }
}

fn assistant_text(events: &[SessionEvent]) -> String {
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

fn pending_approval(events: &[SessionEvent]) -> Option<String> {
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

fn wait_for_approval(client: &Client, session_id: &str) -> String {
    let mut found = None;
    wait_until("an approval request", || {
        found = pending_approval(&read_all(client, session_id));
        found.is_some()
    });
    found.unwrap()
}

/// What a client would hold after reading a subscription: its events, and its streaming tail.
#[derive(Default)]
struct Follower {
    events: Vec<SessionEvent>,
    tail: Tail,
}

impl Follower {
    fn apply(&mut self, item: SubscriptionItem) {
        match item {
            SubscriptionItem::Event(event) => {
                self.tail.on_event(&event.body);
                self.events.push(event);
            }
            SubscriptionItem::Tail(tail) => self.tail = tail,
            SubscriptionItem::Delta(delta) => self.tail.apply(&delta),
            SubscriptionItem::Notice(_) | SubscriptionItem::Disconnected => {}
        }
    }

    fn drain_until(&mut self, subscription: &Subscription, what: &str, mut done: impl FnMut(&Follower) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !done(self) {
            let wait = deadline.saturating_duration_since(Instant::now());
            match subscription.items.recv_timeout(wait) {
                Ok(item) => self.apply(item),
                Err(e) => panic!("timed out waiting for {what}: {e}"),
            }
        }
    }

    fn tail_text(&self) -> String {
        self.tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }
}

// The core loop --------------------------------------------------------------------------------

#[test]
fn a_session_runs_in_a_real_worktree_with_the_worker_as_a_separate_process() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    assert_eq!(git(&worktree, &["branch", "--show-current"]), workspace.branch);
    assert_eq!(workspace.status, WorkspaceStatus::Ready);

    client.session_prompt(&session.id, "say:hello from the agent; tool:pwd && echo made > made.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert_eq!(assistant_text(&events), "hello from the agent");

    // The agent ran in the worktree, not in the repository or the hub's directory.
    assert!(worktree.join("made.txt").exists());
    assert!(!stack.repo().join("made.txt").exists());
    let reported = events.iter().find_map(|e| match &e.body {
        EventBody::AgentInit { cwd, .. } => cwd.clone(),
        _ => None,
    });
    assert_eq!(reported.map(|c| PathBuf::from(c).canonicalize().unwrap()), Some(worktree.canonicalize().unwrap()));

    // hub -> worker -> agent, each its own process.
    let agent_pid = events.iter().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    });
    let agent_pid = agent_pid.expect("an AgentStarted event");
    let parent_of = |pid: u32| -> u32 {
        let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    };
    let worker_pid = parent_of(agent_pid);
    assert_ne!(worker_pid, stack.hub_pid());
    assert_eq!(parent_of(worker_pid), stack.hub_pid());
}

#[test]
fn prompts_are_recorded_before_anything_else_and_retries_are_recorded_once() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let (first, duplicate) = client.session_prompt(&session.id, "say:once", Some("msg-1".into())).unwrap();
    assert!(!duplicate);
    assert_eq!(first, 1);
    let (again, duplicate) = client.session_prompt(&session.id, "say:once", Some("msg-1".into())).unwrap();
    assert!(duplicate);
    assert_eq!(again, first);

    let events = wait_for_turn_end(&client, &session.id, 1);
    let prompts = events.iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
    assert_eq!(assistant_text(&events), "once");
    let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ended, 1, "the agent must only receive it once");
}

#[test]
fn a_prompt_sent_mid_turn_folds_in_and_the_session_stays_running_until_it_is_done() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "slow:10:first", None).unwrap();
    wait_until("the first turn to start", || {
        read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentInit { .. }))
    });
    client.session_prompt(&session.id, "say:second", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let text = assistant_text(&events);
    assert!(text.ends_with("second"), "{text}");
    let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ended, 1, "a folded prompt has no result of its own");
}

// Surviving a client restart -----------------------------------------------------------------------

#[test]
fn work_continues_with_no_client_and_a_new_client_catches_up_with_no_gaps() {
    let stack = Stack::start();
    let (session_id, seen, agent_pid) = {
        let client = stack.client();
        let (_, session) = stack.workspace(&client);
        client.session_prompt(&session.id, "slow:30:chunk", None).unwrap();
        let subscription = client.session_subscribe(&session.id, 1).unwrap();
        let mut follower = Follower::default();
        follower.drain_until(&subscription, "streaming text", |f| f.tail_text().contains("chunk3"));
        let agent_pid = follower
            .events
            .iter()
            .find_map(|e| match &e.body {
                EventBody::AgentStarted { pid, .. } => Some(*pid),
                _ => None,
            })
            .unwrap();
        (session.id, follower.events.len() as u64, agent_pid)
        // The client disconnects here, mid-turn.
    };
    let watcher = stack.client();
    wait_until("the hub to see the client leave", || watcher.hub_status().unwrap().clients == 1);
    assert_eq!(watcher.session_get(&session_id).unwrap().status, SessionStatus::Running);
    assert!(is_alive(agent_pid), "the agent must outlive its client");

    let client = stack.client();
    let events = wait_for_turn_end(&client, &session_id, 1);
    assert_gapless(&events);
    assert!(assistant_text(&events).contains("chunk29"), "the turn finished with nobody attached");

    // Subscribing from the last seq seen delivers exactly the rest.
    let subscription = client.session_subscribe(&session_id, seen + 1).unwrap();
    let mut follower = Follower::default();
    let total = events.len();
    follower.drain_until(&subscription, "the rest of the log", |f| f.events.len() as u64 + seen >= total as u64);
    assert_eq!(follower.events.first().unwrap().seq, seen + 1);
    let mut stitched: Vec<u64> = (1..=seen).collect();
    stitched.extend(follower.events.iter().map(|e| e.seq));
    assert_eq!(stitched, (1..=total as u64).collect::<Vec<_>>());

    // The session is still usable, on the same agent process.
    client.session_prompt(&session_id, "say:still here", None).unwrap();
    let events = wait_for_turn_end(&client, &session_id, 2);
    assert!(assistant_text(&events).ends_with("still here"));
    assert!(!events.iter().skip(total).any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
}

#[test]
fn a_client_attaching_mid_message_sees_the_partial_text_at_once() {
    let stack = Stack::start();
    let first = stack.client();
    let (_, session) = stack.workspace(&first);
    first.session_prompt(&session.id, "slow:40:part", None).unwrap();
    let live = first.session_subscribe(&session.id, 1).unwrap();
    let mut watcher = Follower::default();
    watcher.drain_until(&live, "some streaming", |f| f.tail_text().contains("part5"));

    let second = stack.client();
    let subscription = second.session_subscribe(&session.id, 1).unwrap();
    let mut late = Follower::default();
    // Its first tail should already hold what was streamed, without waiting for more.
    late.drain_until(&subscription, "the tail snapshot", |f| f.tail_text().contains("part5"));

    // And after that, its view stays identical to a client that was there from the start.
    late.drain_until(&subscription, "the turn to end", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    watcher.drain_until(&live, "the turn to end", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    let seqs = |f: &Follower| f.events.iter().map(|e| e.seq).collect::<Vec<_>>();
    let common = seqs(&late).len().min(seqs(&watcher).len());
    assert_eq!(seqs(&late)[..common], seqs(&watcher)[..common]);
    assert_eq!(assistant_text(&late.events), assistant_text(&watcher.events));
    assert!(late.tail.is_empty() && watcher.tail.is_empty());
}

#[test]
fn every_chunk_reaches_the_tail_exactly_once_while_streaming() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "slow:8:x", None).unwrap();
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    let mut follower = Follower::default();
    follower.drain_until(&subscription, "the last chunk", |f| f.tail_text().contains("x7"));
    assert_eq!(follower.tail_text(), "x0 x1 x2 x3 x4 x5 x6 x7 ");
}

/// Sets a flag even if the test is panicking, so a scoped thread watching it still finishes.
struct SetOnDrop<'a>(&'a AtomicBool);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[test]
fn clients_attaching_during_a_burst_never_see_a_chunk_twice_or_miss_one() {
    const CHUNKS: u32 = 40_000;
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let expected: String = (0..CHUNKS).map(|i| i.to_string() + " ").collect();

    // Attach and detach over and over while deltas pour in. A chunk could land both in a client's
    // snapshot and in the live stream after it, or in neither. Whenever it's checked, a client's
    // tail must be an exact prefix of the text being written: a lagging client can be short, but
    // nothing else can differ.
    let done = AtomicBool::new(false);
    let (snapshots_mid_stream, attaches) = std::thread::scope(|scope| {
        let attachers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut snapshots_mid_stream = 0u32;
                    let mut attaches = 0u32;
                    while !done.load(Ordering::Relaxed) {
                        let client = stack.client();
                        let subscription = client.session_subscribe(&session.id, 1).unwrap();
                        attaches += 1;
                        let mut tail = Tail::default();
                        for _ in 0..50 {
                            let Ok(item) = subscription.items.recv_timeout(Duration::from_millis(50)) else { break };
                            match item {
                                SubscriptionItem::Delta(delta) => tail.apply(&delta),
                                SubscriptionItem::Tail(snapshot) => {
                                    snapshots_mid_stream += u32::from(!snapshot.is_empty());
                                    tail = snapshot;
                                }
                                SubscriptionItem::Event(event) => tail.on_event(&event.body),
                                SubscriptionItem::Notice(_) => {}
                                SubscriptionItem::Disconnected => panic!("lost the hub"),
                            }
                        }
                        let held: String =
                            tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect();
                        assert!(
                            expected.starts_with(&held),
                            "tail diverged after {} of {} bytes",
                            common_prefix(&expected, &held),
                            held.len()
                        );
                    }
                    (snapshots_mid_stream, attaches)
                })
            })
            .collect();
        let finish = SetOnDrop(&done);
        client.session_prompt(&session.id, &format!("burst:{CHUNKS}"), None).unwrap();
        let events = wait_for_turn_end(&client, &session.id, 1);
        assert_eq!(assistant_text(&events), expected, "the durable message is the whole text");
        drop(finish);
        attachers
            .into_iter()
            .map(|attacher| attacher.join().unwrap())
            .fold((0, 0), |totals, one| (totals.0 + one.0, totals.1 + one.1))
    });
    // Otherwise this test proved nothing.
    assert!(snapshots_mid_stream >= 10, "only {snapshots_mid_stream} of {attaches} attaches landed mid-stream");
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

#[test]
fn a_client_that_stalled_during_a_message_ends_with_nothing_left_in_its_tail() {
    use protocol::client::{ClientFrame, HubFrame};
    use std::io::{BufRead, BufReader, Write};
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    // A client that subscribes, then doesn't read its socket until everything has finished.
    let mut stream = std::os::unix::net::UnixStream::connect(stack.socket()).unwrap();
    let requests = [
        ClientRequest::Hello { protocol_version: protocol::PROTOCOL_VERSION, client: "stalled".into() },
        ClientRequest::SessionSubscribe { id: session.id.clone(), from_seq: 1, subscription_id: 7 },
    ];
    for (id, request) in requests.into_iter().enumerate() {
        stream.write_all(&protocol::frame::encode(&ClientFrame { id: id as u64, request }).unwrap()).unwrap();
    }
    // Many short messages put events and deltas side by side in the live stream, and then the agent
    // dies mid-message. An event read ahead of the deltas before it would let them land after the
    // agent's exit, and nothing would ever clear them. Sized to overflow the connection's queue, so
    // the hub falls behind, without overflowing the live buffer, which would resync instead.
    let mut script: Vec<String> = (0..500).map(|i| format!("say:message {i}")).collect();
    script.extend(["partial:cut off".to_string(), "crash:3".to_string()]);
    client.session_prompt(&session.id, &script.join("; "), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);

    stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut reader = BufReader::new(stream);
    let mut tail = Tail::default();
    let mut line = String::new();
    let mut ended = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            break;
        }
        match serde_json::from_str::<HubFrame>(&line).unwrap() {
            HubFrame::Event { event, .. } => {
                ended |= matches!(event.body, EventBody::AgentExited { .. });
                tail.on_event(&event.body);
            }
            HubFrame::Delta { delta, .. } => tail.apply(&delta),
            HubFrame::Tail { tail: snapshot, .. } => tail = snapshot,
            HubFrame::Reply { .. } | HubFrame::Notice { .. } => {}
        }
    }
    assert!(ended, "the client should have read through the agent's exit");
    assert!(tail.is_empty(), "stale partial text would stay on screen: {} message(s)", tail.messages.len());
}

#[test]
fn status_is_already_current_when_an_event_is_delivered() {
    // A client that sees the agent go idle and then asks for status must not read an old value:
    // with nothing more coming, it would wait forever.
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    let mut idles = 0;
    client.session_prompt(&session.id, "say:0", None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while idles < 200 {
        let wait = deadline.saturating_duration_since(Instant::now());
        let SubscriptionItem::Event(event) = subscription.items.recv_timeout(wait).expect("more events") else {
            continue;
        };
        // The event that moves the status from running to idle.
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle, "after {idles} turns");
            idles += 1;
            client.session_prompt(&session.id, &format!("say:{idles}"), None).unwrap();
        }
    }
}

// Approvals ------------------------------------------------------------------------------------------

#[test]
fn an_approval_can_be_answered_by_a_different_client_after_a_restart() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo approved > approved.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    wait_for_status(&client, &session.id, SessionStatus::WaitingApproval);
    let worktree = PathBuf::from(workspace.path.unwrap());
    assert!(!worktree.join("approved.txt").exists(), "must not run before it's approved");
    drop(client);

    let other = stack.client();
    other
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap();
    let events = wait_for_turn_end(&other, &session.id, 1);
    assert!(worktree.join("approved.txt").exists());
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::ApprovalResolved { approval_id: id, outcome: ApprovalOutcome::Allowed } if *id == approval_id
    )));

    // Answering twice is an error rather than sending the agent a second response.
    let err = other
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
}

#[test]
fn a_denied_tool_does_not_run_and_the_agent_hears_why() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo nope > nope.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    client
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Deny { message: "not that".into(), interrupt: false },
        )
        .unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(!PathBuf::from(workspace.path.unwrap()).join("nope.txt").exists());
    let told = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { text, is_error: true, .. } if text == "not that")),
        _ => false,
    });
    assert!(told);
}

#[test]
fn an_approved_edit_to_a_tools_input_is_what_runs() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo original > out.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    let decision = ApprovalDecision::Allow {
        updated_input: Some(serde_json::json!({"command": "echo edited > out.txt"})),
        updated_permissions: None,
    };
    client.session_approve(&session.id, &approval_id, decision).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let out = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("out.txt")).unwrap();
    assert_eq!(out, "edited\n");
}

// Cancelling ---------------------------------------------------------------------------------------

#[test]
fn cancelling_stops_the_turn_and_keeps_the_session_usable() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    wait_until("the tool to start", || {
        read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::Assistant { .. }))
    });
    client.session_cancel(&session.id).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, .. })));

    client.session_prompt(&session.id, "say:after", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("after"));
}

#[test]
fn cancelling_closes_an_open_approval() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo x", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    client.session_cancel(&session.id).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::ApprovalResolved { approval_id: id, outcome: ApprovalOutcome::Cancelled } if *id == approval_id
    )));
    assert_eq!(pending_approval(&events), None);
}

#[test]
fn cancelling_with_nothing_running_is_harmless() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_cancel(&session.id).unwrap();
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

// Processes going away -------------------------------------------------------------------------------

#[test]
fn a_hub_crash_stops_the_agent_and_the_next_hub_resumes_the_conversation() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "say:remember mango; tool:echo x", None).unwrap();
    wait_for_approval(&client, &session.id);
    let events = read_all(&client, &session.id);
    let agent_pid = events
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();
    drop(client);

    stack.kill_hub();
    wait_until("the agent to stop after the hub died", || !is_alive(agent_pid));

    stack.start_hub();
    let client = stack.client();
    let recovered = client.session_get(&session.id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert_eq!(pending_approval(&events), None, "nobody can answer an approval for a dead agent");
    assert!(events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::HubRestart, .. })));

    let before = events.len();
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    let later = &events[before..];
    assert!(later.iter().any(|e| matches!(e.body, EventBody::AgentStarted { resumed: true, .. })));
    assert!(assistant_text(later).contains("remember mango"), "{}", assistant_text(later));
}

#[test]
fn a_clean_hub_stop_stops_workers_and_agents_and_the_next_hub_resumes() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    let agent_pid = events
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();

    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(!is_alive(agent_pid));
    // Well under the 15 s the hub waits for a stuck worker, so this was a clean stop.
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
    assert!(!stack.socket().exists(), "the socket should be removed");

    stack.start_hub();
    let client = stack.client();
    // It was idle when stopped, so there's nothing to report as interrupted.
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).contains("say:first"));
}

#[test]
fn stopping_a_hub_with_several_workers_is_quick() {
    let mut stack = Stack::start();
    let client = stack.client();
    for _ in 0..4 {
        let (workspace, _) = stack.workspace(&client);
        client.workspace_status(&workspace.id).unwrap();
    }
    assert_eq!(client.hub_status().unwrap().workers, 4);
    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
}

#[test]
fn a_worker_crash_is_reported_and_the_next_prompt_gets_a_new_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent_pid = None;
    wait_until("the agent to start", || {
        agent_pid = read_all(&client, &session.id).iter().find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        });
        agent_pid.is_some()
    });
    let agent_pid = agent_pid.unwrap();
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &agent_pid.to_string()]).output().unwrap();
    let worker_pid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    // SAFETY: kill(2) has no memory effects. This is the worker's PID, read just above.
    unsafe { libc::kill(worker_pid, libc::SIGKILL) };

    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert!(events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::WorkerLost, .. })));

    wait_until("the agent to stop along with its worker", || !is_alive(agent_pid));

    client.session_prompt(&session.id, "say:back", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert!(assistant_text(&events).ends_with("back"));
}

#[test]
fn an_agent_dying_during_an_approval_closes_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo x", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    let agent_pid = read_all(&client, &session.id)
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();
    signal(agent_pid, libc::SIGKILL);

    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert_eq!(pending_approval(&events), None);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::Unexpected, signal: Some(9), .. }))
    );
    let err = client
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
}

#[test]
fn an_agent_crash_is_reported_and_the_next_prompt_resumes() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; crash:3", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(3), .. }))
    );

    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("say:before"), "{}", assistant_text(&events));
}

#[test]
fn a_missing_agent_binary_is_reported_in_the_log_rather_than_hanging() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    command.env("TRESTLE_CLAUDE_BIN", "/nonexistent/claude");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hi", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    let reported = events.iter().any(|e| match &e.body {
        EventBody::AgentExited { reason: ExitReason::StartFailed, stderr_tail, .. } => {
            stderr_tail.iter().any(|line| line.contains("/nonexistent/claude"))
        }
        _ => false,
    });
    assert!(reported, "{events:#?}");
}

#[test]
fn killing_the_hub_and_worker_together_cannot_lead_to_two_agents() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first; hang:60000", None).unwrap();
    let mut agent_pid = 0;
    wait_until("the tool to start", || {
        let events = read_all(&client, &session.id);
        agent_pid = started_pid(&events).unwrap_or(0);
        agent_pid != 0 && assistant_text(&events).contains("first")
    });
    let worker_pid = parent_of(agent_pid);
    drop(client);
    signal(worker_pid, libc::SIGKILL);
    stack.kill_hub();
    assert!(is_alive(agent_pid), "the fake keeps hanging after its stdin closes, like a real long tool");

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    wait_until("the second turn to finish", || assistant_text(&read_all(&client, &session.id)).ends_with("second"));
    assert!(!is_alive(agent_pid), "the old agent must be gone before a new one runs on its conversation");
}

#[test]
fn an_exit_reported_late_cannot_be_mistaken_for_the_agent_that_replaced_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    // Something it started keeps its stdout open, so the worker takes seconds to report the exit.
    client.session_prompt(&session.id, "say:first; holdpipe:7; crash:3", None).unwrap();
    let mut first = None;
    wait_until("the agent to start", || {
        first = latest_started_pid(&read_all(&client, &session.id));
        first.is_some()
    });
    let first = first.unwrap();
    wait_until("the agent to die", || !is_alive(first));
    let exits =
        |events: &[SessionEvent]| events.iter().filter(|e| matches!(e.body, EventBody::AgentExited { .. })).count();
    assert_eq!(exits(&read_all(&client, &session.id)), 0, "the exit should still be on its way");

    // A prompt in that window goes to a new agent, rather than failing on the dead one's pipe.
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut second = 0;
    wait_until("a new agent to start", || {
        second = latest_started_pid(&read_all(&client, &session.id)).unwrap_or(first);
        second != first
    });
    wait_until("the old exit to be reported", || exits(&read_all(&client, &session.id)) == 1);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert!(!events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. })));

    // It's in the log, but it wasn't about the agent running now.
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Running);
    assert!(is_alive(second));

    // The hub still knows about the new agent, so losing its worker is noticed and it gets stopped.
    signal(parent_of(second), libc::SIGKILL);
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    wait_until("the new agent to stop too", || !is_alive(second));
}

#[test]
fn a_prompt_sent_as_the_agent_dies_is_delivered_to_a_new_one() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let mut seen = 0;
    for trial in 0..3 {
        // A burst leaves output queued behind the process, which keeps its exit from being noticed.
        client.session_prompt(&session.id, "burst:20000; crash:3", None).unwrap();
        // The agent from the last trial takes this prompt, or a new one on the first.
        let mut agent_pid = None;
        wait_until("an agent to be running", || {
            agent_pid = latest_started_pid(&read_all(&client, &session.id));
            agent_pid.is_some()
        });
        wait_until("the agent to die", || !is_alive(agent_pid.unwrap()));
        let marker = format!("after-{trial}");
        client.session_prompt(&session.id, &format!("say:{marker}"), None).unwrap();

        let mut events = Vec::new();
        wait_until("the prompt to be answered", || {
            events = read_all(&client, &session.id);
            assistant_text(&events[seen..]).contains(&marker)
        });
        assert_gapless(&events);
        let failed =
            events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. }));
        assert!(!failed, "trial {trial} reported a failed start");
        wait_for_status(&client, &session.id, SessionStatus::Idle);
        seen = events.len();
    }

    // The hub must still know about the agent it's on now, so losing it is noticed.
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    wait_until("the tool to start", || read_all(&client, &session.id).len() > seen + 2);
    let agent_pid = latest_started_pid(&read_all(&client, &session.id)).unwrap();
    signal(parent_of(agent_pid), libc::SIGKILL);
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
}

// A worker binary that misbehaves --------------------------------------------------------------------

/// Runs the hub from a copy of its binary, so a test can replace what it starts next.
fn hub_from_copy(stack: &mut Stack) -> PathBuf {
    stack.kill_hub();
    let exe = stack.path("trestled-copy");
    install_executable(Path::new(env!("CARGO_BIN_EXE_trestled")), &exe);
    stack.hub = Some(stack.command_at(&exe).spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());
    exe
}

/// Through `install`, because writing an executable from this multithreaded process can break running
/// it on Linux. A fork in another test briefly inherits the open file, and `exec` then fails with
/// "text file busy". This way, this process never has the file open for writing.
fn install_executable(source: &Path, target: &Path) {
    let _ = std::fs::remove_file(target);
    let status = Command::new("install").args(["-m", "755"]).arg(source).arg(target).status().unwrap();
    assert!(status.success());
}

fn restore_real_worker(exe: &Path) {
    install_executable(Path::new(env!("CARGO_BIN_EXE_trestled")), exe);
}

fn replace_with_script(exe: &Path, body: &str) {
    let source = exe.with_extension("source");
    std::fs::write(&source, format!("#!/bin/sh\n{body}\n")).unwrap();
    install_executable(&source, exe);
}

/// Kills every worker of this hub, and waits until the hub has noticed.
fn kill_workers(stack: &Stack, client: &Client) {
    let out = Command::new("pgrep").args(["-P", &stack.hub_pid().to_string()]).output().unwrap();
    for pid in String::from_utf8_lossy(&out.stdout).split_whitespace() {
        // SAFETY: kill(2) has no memory effects. These are the hub's own children.
        unsafe { libc::kill(pid.parse().unwrap(), libc::SIGKILL) };
    }
    wait_until("the hub to notice", || client.hub_status().unwrap().workers == 0);
}

fn answered_within<T: Send + 'static>(limit: Duration, call: impl FnOnce() -> T + Send + 'static) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call());
    });
    rx.recv_timeout(limit).is_ok()
}

#[test]
fn a_worker_speaking_another_version_only_blocks_until_it_is_replaced() {
    let mut stack = Stack::start();
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.workspace_status(&workspace.id).unwrap();

    replace_with_script(
        &exe,
        r#"read line
printf '%s\n' '{"frame":"reply","id":1,"error":{"code":"version_mismatch","message":"stub speaks another version"}}'
cat > /dev/null"#,
    );
    kill_workers(&stack, &client);

    let err = client.workspace_status(&workspace.id).unwrap_err();
    assert!(err.to_string().contains("another version"), "{err}");
    let err = client.workspace_status(&workspace.id).unwrap_err();
    assert!(err.to_string().contains("another version"), "the same error, not a stuck instance: {err}");

    restore_real_worker(&exe);
    client.workspace_status(&workspace.id).unwrap();
    client.session_prompt(&session.id, "say:recovered", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 1)).ends_with("recovered"));
}

#[test]
fn a_worker_that_never_answers_only_affects_its_own_workspace_and_times_out() {
    let mut stack = Stack::start();
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (hung, _) = stack.workspace(&client);
    let (also_hung, _) = stack.workspace(&client);
    let (fine, _) = stack.workspace(&client);
    kill_workers(&stack, &client);
    client.workspace_status(&fine.id).unwrap();

    // The next workers to start never say hello.
    replace_with_script(&exe, "exec sleep 1000");
    let started = Instant::now();
    let blocked: Vec<_> = [hung.id.clone(), also_hung.id.clone()]
        .into_iter()
        .map(|id| {
            let socket = stack.socket();
            std::thread::spawn(move || {
                Client::connect(&socket, "test").unwrap().workspace_status(&id).map(|_| ()).map_err(|e| e.to_string())
            })
        })
        .collect();
    std::thread::sleep(Duration::from_millis(300));

    let socket = stack.socket();
    assert!(
        answered_within(Duration::from_secs(3), move || Client::connect(&socket, "test")
            .unwrap()
            .hub_status()
            .unwrap()),
        "hub status must not wait on one workspace's worker"
    );
    let socket = stack.socket();
    let fine_id = fine.id.clone();
    assert!(
        answered_within(Duration::from_secs(3), move || Client::connect(&socket, "test")
            .unwrap()
            .workspace_status(&fine_id)
            .unwrap()),
        "other workspaces must not wait on it either"
    );

    for attempt in blocked {
        assert!(attempt.join().unwrap().is_err(), "a worker that never answers is an error");
    }
    // Each times out on its own 10 s. If one start had to wait for the other, this would be 20 s.
    assert!(started.elapsed() < Duration::from_secs(15), "took {:?}", started.elapsed());

    restore_real_worker(&exe);
    client.workspace_status(&hung.id).unwrap();
}

// Launch flags disagreeing with the CLI's records ---------------------------------------------------

#[test]
fn a_deleted_conversation_starts_fresh_with_a_warning() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);
    stack.kill_hub();
    // The hub thinks this conversation exists, because it saw it open.
    std::fs::remove_file(stack.agent_state().join(&session.id)).unwrap();

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("second"));
    let warned =
        events.iter().any(|e| matches!(&e.body, EventBody::Notice { message, .. } if message.contains("not found")));
    assert!(warned, "{events:#?}");
}

#[test]
fn a_conversation_the_hub_did_not_know_had_opened_is_continued() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    drop(client);
    stack.kill_hub();
    // As if a launch saved the conversation, then crashed before the hub heard about it.
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join(&session.id), "say:from before the crash\n").unwrap();

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("from before the crash"), "{events:#?}");
}

// Workspaces -------------------------------------------------------------------------------------------

#[test]
fn archiving_protects_uncommitted_work_keeps_the_branch_and_restoring_brings_it_all_back() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    client.session_prompt(&session.id, "tool:echo work > work.txt; say:done", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    assert!(worktree.join("work.txt").exists());
    client.session_prompt(&session.id, "say:the agent was left running by the refused archive", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert!(!worktree.exists());
    assert_eq!(client.workspace_list(false).unwrap().len(), 0);
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert!(!git(&stack.repo(), &["branch", "--list", &workspace.branch]).is_empty());

    // An archived workspace takes no prompts, and its history stays readable.
    let err = client.session_prompt(&session.id, "say:no", None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    let before = read_all(&client, &session.id);
    assert!(before.len() > 3);

    let restored = client.workspace_restore(&workspace.id).unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(std::fs::read_to_string(worktree.join("work.txt")).unwrap(), "work\n");
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 3);
    assert!(assistant_text(&events[before.len()..]).contains("tool:echo work"));
}

#[test]
fn archiving_keeps_a_branch_the_agent_renamed_and_restoring_finds_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:git branch -m fix-the-login-bug; say:renamed", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.branch, "fix-the-login-bug");
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let worktree = PathBuf::from(restored.path.unwrap());
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "fix-the-login-bug");
}

#[test]
fn a_forced_archive_stops_a_running_agent() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent_pid = None;
    wait_until("the tool to start", || {
        let events = read_all(&client, &session.id);
        agent_pid = events.iter().find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        });
        agent_pid.is_some() && events.iter().any(|e| matches!(e.body, EventBody::Assistant { .. }))
    });
    std::fs::write(PathBuf::from(workspace.path.clone().unwrap()).join("dirty.txt"), "x").unwrap();
    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(!is_alive(agent_pid.unwrap()));
    assert!(!PathBuf::from(workspace.path.unwrap()).exists());
    let session = client.session_get(&session.id).unwrap();
    assert_eq!(session.status, SessionStatus::Interrupted);
}

#[test]
fn status_and_diff_come_from_the_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:echo changed >> a.txt && echo new > new.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let status = client.workspace_status(&workspace.id).unwrap();
    assert_eq!(status.branch.as_deref(), Some(workspace.branch.as_str()));
    let file = |path: &str| status.files.iter().find(|f| f.path == path).unwrap().status.clone();
    assert_eq!(file("a.txt"), ".M");
    assert_eq!(file("new.txt"), "??");

    let diff = client.workspace_diff(&workspace.id).unwrap();
    assert!(diff.patch.contains("+changed"), "{}", diff.patch);
    assert!(diff.patch.contains("+new"), "{}", diff.patch);
}

#[test]
fn comments_are_kept_by_the_hub_and_survive_a_restart() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let added = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this").unwrap();
    assert!(client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "  ").is_err(), "an empty comment");
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![added.clone()]);
    client.comment_delete(&added.id).unwrap();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![]);
}

#[test]
fn a_workspace_with_no_pull_request_reports_none_rather_than_failing() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(client.workspace_pull_request(&workspace.id).unwrap(), None);
}

#[test]
fn a_workspace_reports_its_pull_request_from_gh() {
    let mut stack = Stack::start();
    stack.kill_hub();
    // No login shell, so this PATH is the one used, with a stand-in `gh` at the front of it.
    let bin = stack.path("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let source = stack.path("gh.source");
    std::fs::write(
        &source,
        "#!/bin/sh\nprintf '{\"isDraft\":false,\"number\":7,\"state\":\"OPEN\",\"title\":\"T\",\"url\":\"https://example.com/7\"}'\n",
    )
    .unwrap();
    install_executable(&source, &bin.join("gh"));
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut command = stack.command();
    command.env("PATH", path).env("SHELL", "/usr/bin/false");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let pr = client.workspace_pull_request(&workspace.id).unwrap().unwrap();
    assert_eq!((pr.number, pr.state.as_str(), pr.url.as_str()), (7, "OPEN", "https://example.com/7"));
}

// Model and approval mode --------------------------------------------------------------------------

#[test]
fn a_change_reaches_the_running_agent_and_the_next_launch() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo first > first.txt", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let changed = client.session_configure(&session.id, Some("opus".into()), Some("bypassPermissions".into())).unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("bypassPermissions"));
    assert_eq!(changed.model.as_deref(), Some("opus"));

    // The same process, and it no longer asks.
    client.session_prompt(&session.id, "tool:echo second > second.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let path = PathBuf::from(workspace.path.clone().unwrap());
    assert!(path.join("second.txt").exists());
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count(), 1);
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    let last_init = events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentInit { model, permission_mode, .. } => Some((model.clone(), permission_mode.clone())),
        _ => None,
    });
    assert_eq!(last_init, Some((Some("opus".to_string()), Some("bypassPermissions".to_string()))));

    // A new process starts with what was chosen, from the hub's own record.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("bypassPermissions"));
    client.session_prompt(&session.id, "tool:echo third > third.txt", None).unwrap();
    wait_until("the third file", || path.join("third.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions") && args.contains("--model opus"), "{args}");
}

#[test]
fn a_change_with_no_agent_running_just_saves_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, None);
    let changed = client.session_configure(&session.id, None, Some("plan".into())).unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("plan"));
    assert_eq!(changed.model, None, "None leaves a setting as it is");
    let changed = client.session_configure(&session.id, Some("haiku".into()), None).unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("plan"), "None leaves a setting as it is");
    let saved = client.session_get(&session.id).unwrap();
    assert_eq!((saved.model.as_deref(), saved.permission_mode.as_deref()), (Some("haiku"), Some("plan")));
}

#[test]
fn a_change_the_agent_refuses_changes_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:ready", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let refused = client.session_configure(&session.id, Some("not-a-model".into()), None).unwrap_err();
    assert!(refused.to_string().contains("not found"), "{refused}");
    client.session_configure(&session.id, None, Some("nonsense".into())).unwrap_err();
    let after = client.session_get(&session.id).unwrap();
    assert_eq!((after.model, after.permission_mode.as_deref()), (None, Some("bypassPermissions")));

    client.session_prompt(&session.id, "say:still fine", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("still fine"));
}

#[test]
fn approving_a_plan_leaves_plan_mode_and_the_next_launch_agrees() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("plan"));
    client.session_prompt(&session.id, "plan:Do the thing", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let asked = read_all(&client, &session.id).into_iter().find_map(|e| match e.body {
        EventBody::ApprovalRequested { tool_name, input, .. } => Some((tool_name, input)),
        _ => None,
    });
    let (tool_name, input) = asked.unwrap();
    assert_eq!((tool_name.as_str(), input["plan"].as_str()), ("ExitPlanMode", Some("Do the thing")));
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("plan"));

    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("manual"));

    // The CLI calls this mode `default` while running and only takes `manual` on launch.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:back", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("back"));
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode manual"), "{args}");
}

#[test]
fn a_mode_chosen_during_setup_applies_to_the_prompt_waiting_on_it() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 2\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo done > done.txt", None).unwrap();

    let started = Instant::now();
    client.session_configure(&session.id, None, Some("bypassPermissions".into())).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "waited for setup: {:?}", started.elapsed());
    assert_eq!(client.workspace_list(false).unwrap()[0].setup, protocol::model::SetupStatus::Running);

    let path = PathBuf::from(workspace.path.unwrap());
    wait_until("the tool to run without asking", || path.join("done.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}

// Finding a hub --------------------------------------------------------------------------------------

/// The desktop app starts a hub from a background thread with every signal blocked, and that carries
/// across exec. Blocking `SIGCHLD` alone is enough to make the first `git` hang.
#[test]
fn a_hub_started_with_every_signal_blocked_still_works() {
    use std::os::unix::process::CommandExt as _;

    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    // SAFETY: both are async-signal-safe, and the set is initialized by the first.
    unsafe {
        command.pre_exec(|| {
            let mut all = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            libc::sigfillset(all.as_mut_ptr());
            libc::pthread_sigmask(libc::SIG_SETMASK, all.as_ptr(), std::ptr::null_mut());
            Ok(())
        });
    }
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub to accept connections", || Client::connect(&stack.socket(), "test").is_ok());
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    // On its own thread, because what this checks for is a hang.
    let (created, result) = std::sync::mpsc::channel();
    let creating = client.clone();
    std::thread::spawn(move || {
        let _ = created.send(creating.workspace_create(&repo.id, None, None));
    });
    let workspace = result.recv_timeout(Duration::from_secs(20)).expect("creating a workspace hung").unwrap();
    assert_eq!(workspace.status, WorkspaceStatus::Ready);

    // An agent started under it works too, and stopping it by signal still does.
    let session =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into())).unwrap();
    client.session_prompt(&session.id, "tool:echo done > done.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert!(PathBuf::from(workspace.path.unwrap()).join("done.txt").exists());
    drop(client);
    let mut hub = stack.hub.take().unwrap();
    terminate(hub.id());
    wait_until("the hub to stop on SIGTERM", || !matches!(hub.try_wait(), Ok(None)));
}

#[test]
fn a_hub_that_drops_the_connection_before_reading_it_is_replaced() {
    replaces_a_hub_that_drops_the_connection(false);
}

#[test]
fn a_hub_that_drops_the_connection_after_reading_it_is_replaced() {
    replaces_a_hub_that_drops_the_connection(true);
}

/// The client finds out differently depending on how far it got: a failed write, or no reply.
fn replaces_a_hub_that_drops_the_connection(reads_hello_first: bool) {
    let mut stack = Stack::start();
    stack.kill_hub();
    let socket = stack.socket();
    let _ = std::fs::remove_file(&socket);
    let dying = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let path = socket.clone();
    let hub_going_away = std::thread::spawn(move || {
        let (connection, _) = dying.accept().unwrap();
        if reads_hello_first {
            let mut hello = String::new();
            std::io::BufRead::read_line(&mut std::io::BufReader::new(&connection), &mut hello).unwrap();
        }
        // In this order, so the socket has gone before the client finds out and starts a real hub.
        drop(dying);
        std::fs::remove_file(path).unwrap();
        drop(connection);
    });

    let home = stack.path("home");
    let workspaces = stack.path("workspaces");
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(home.to_str().unwrap().to_string()),
        protocol::paths::ENV_WORKSPACES => Some(workspaces.to_str().unwrap().to_string()),
        _ => None,
    })
    .unwrap();
    let client = Client::connect_or_start(&paths, Path::new(env!("CARGO_BIN_EXE_trestled")), "test").unwrap();
    hub_going_away.join().unwrap();
    assert!(client.hub_status().is_ok());
    client.hub_shutdown().unwrap();
}

// Questions from the agent --------------------------------------------------------------------------

#[test]
fn an_answer_gets_back_to_the_agent_in_every_mode() {
    let stack = Stack::start();
    let client = stack.client();
    // It asks even with full access, because only a person can answer it.
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:Red or blue?=Red/Blue", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers = std::iter::once(("Red or blue?".to_string(), "Blue".to_string())).collect();
    client.session_approve(&session.id, &approval, ApprovalDecision::Answer { answers }).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let told = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains(r#""Red or blue?"=Blue"#))),
        _ => false,
    });
    assert!(told, "{events:?}");
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

// Several chats in one workspace --------------------------------------------------------------------

#[test]
fn chats_in_a_workspace_run_side_by_side_and_keep_to_themselves() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, first) = stack.workspace(&client);
    let second =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into())).unwrap();
    client.session_prompt(&first.id, "slow:15:from the first", None).unwrap();
    client.session_prompt(&second.id, "slow:15:from the second", None).unwrap();
    let first_events = wait_for_turn_end(&client, &first.id, 1);
    let second_events = wait_for_turn_end(&client, &second.id, 1);
    assert!(assistant_text(&first_events).contains("first") && !assistant_text(&first_events).contains("second"));
    assert!(assistant_text(&second_events).contains("second") && !assistant_text(&second_events).contains("first"));
    assert_ne!(started_pid(&first_events), started_pid(&second_events));
    let when = |events: &[SessionEvent], wanted: fn(&EventBody) -> bool| {
        events.iter().find(|e| wanted(&e.body)).unwrap().ts_ms
    };
    let started: fn(&EventBody) -> bool = |body| matches!(body, EventBody::AgentStarted { .. });
    let ended: fn(&EventBody) -> bool = |body| matches!(body, EventBody::TurnEnded { .. });
    assert!(when(&first_events, started) < when(&second_events, ended), "one waited for the other");
    assert!(when(&second_events, started) < when(&first_events, ended), "one waited for the other");

    // The first prompt names a chat, and later ones don't rename it.
    client.session_prompt(&first.id, "say:again", None).unwrap();
    wait_for_turn_end(&client, &first.id, 2);
    assert_eq!(client.session_get(&first.id).unwrap().title.as_deref(), Some("slow:15:from the first"));
}

#[test]
fn closing_a_chat_stops_only_its_agent_and_reopening_continues_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, closing) = stack.workspace(&client);
    let staying =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into())).unwrap();
    client.session_prompt(&closing.id, "say:remember me", None).unwrap();
    client.session_prompt(&staying.id, "say:here", None).unwrap();
    let closing_pid = started_pid(&wait_for_turn_end(&client, &closing.id, 1)).unwrap();
    let staying_pid = started_pid(&wait_for_turn_end(&client, &staying.id, 1)).unwrap();

    let closed = client.session_close(&closing.id).unwrap();
    assert!(closed.closed_at_ms.is_some());
    wait_until("its agent to exit", || !is_alive(closing_pid));
    assert!(is_alive(staying_pid), "the other chat's agent went too");

    let refused = client.session_prompt(&closing.id, "say:hello?", None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    let listed = client.session_list(&workspace.id).unwrap();
    assert_eq!(listed.iter().find(|s| s.id == closing.id).unwrap().closed_at_ms, closed.closed_at_ms);

    assert_eq!(client.session_reopen(&closing.id).unwrap().closed_at_ms, None);
    client.session_prompt(&closing.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &closing.id, 2);
    assert!(assistant_text(&events).contains("remember me"), "{}", assistant_text(&events));
}

#[test]
fn closing_a_chat_with_a_prompt_waiting_on_setup_never_starts_its_agent() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 1\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:echo no > never.txt", None).unwrap();
    client.session_close(&session.id).unwrap();
    wait_until("setup to finish", || {
        client.workspace_list(false).unwrap()[0].setup != protocol::model::SetupStatus::Running
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(!PathBuf::from(workspace.path.unwrap()).join("never.txt").exists());
    assert!(!read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
}

// Setup ----------------------------------------------------------------------------------------------

/// Commits Conductor-style settings to the test repository.
fn commit_settings(stack: &Stack, toml: &str) {
    let dir = stack.repo().join(".conductor");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.toml"), toml).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "settings"]);
}

fn wait_for_setup(client: &Client, id: &str) -> protocol::model::SetupStatus {
    let mut status = protocol::model::SetupStatus::Running;
    wait_until("setup to finish", || {
        status = client.workspace_list(true).unwrap().into_iter().find(|w| w.id == id).unwrap().setup;
        !matches!(status, protocol::model::SetupStatus::Running | protocol::model::SetupStatus::NotStarted)
    });
    status
}

#[test]
fn a_new_workspace_gets_its_local_files_and_runs_the_setup_script_with_conductors_variables() {
    let stack = Stack::start();
    std::fs::write(stack.repo().join(".gitignore"), ".env*\n").unwrap();
    std::fs::write(stack.repo().join(".env.local"), "SECRET=1\n").unwrap();
    git(&stack.repo(), &["add", ".gitignore"]);
    git(&stack.repo(), &["commit", "-qm", "ignore"]);
    commit_settings(
        &stack,
        r#"[scripts]
setup = """
test -f .env.local && echo found-env
echo "$CONDUCTOR_WORKSPACE_NAME $CONDUCTOR_PORT $CONDUCTOR_ROOT_PATH" > setup-ran.txt
"""
"#,
    );
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Succeeded);

    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    assert_eq!(std::fs::read_to_string(worktree.join(".env.local")).unwrap(), "SECRET=1\n");
    let ran = std::fs::read_to_string(worktree.join("setup-ran.txt")).unwrap();
    assert!(ran.starts_with(&format!("{} 55000 ", workspace.name)), "{ran}");
    assert!(ran.trim_end().ends_with("repo"), "{ran}");
    let log = client.workspace_setup_log(&workspace.id).unwrap();
    assert!(log.contains("Copied .env.local") && log.contains("found-env"), "{log}");
}

#[test]
fn a_prompt_sent_during_setup_is_recorded_at_once_and_delivered_when_it_finishes() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 1; touch ready.txt\"\n");
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (seq, _) = client.session_prompt(&session.id, "tool:test -f ready.txt && echo was-ready", None).unwrap();
    assert_eq!(seq, 1);
    assert!(!read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
    let events = wait_for_turn_end(&client, &session.id, 1);
    let saw_it = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => {
            blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains("was-ready")))
        }
        _ => false,
    });
    assert!(saw_it, "{events:#?}");
}

#[test]
fn a_failed_setup_warns_once_and_leaves_the_workspace_usable() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"echo nope; exit 7\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Failed { code: Some(7) });
    assert!(client.workspace_setup_log(&workspace.id).unwrap().contains("nope"));

    client.session_prompt(&session.id, "say:one", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    client.session_prompt(&session.id, "say:two", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("two"));
    let warnings = events
        .iter()
        .filter(|e| matches!(&e.body, EventBody::Notice { message, .. } if message.contains("setup")))
        .count();
    assert_eq!(warnings, 1);
}

#[test]
fn stopping_interrupts_a_running_setup_and_it_can_be_run_again() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"touch started.txt; sleep 60\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_until("the script to start", || worktree.join("started.txt").exists());
    client.session_prompt(&session.id, "say:hello", None).unwrap();

    client.session_cancel(&session.id).unwrap();
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Failed { code: None });
    // The waiting prompt goes through, with the warning.
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("hello"));

    std::fs::remove_file(worktree.join("started.txt")).unwrap();
    client.workspace_rerun_setup(&workspace.id).unwrap();
    wait_until("the script to start again", || worktree.join("started.txt").exists());
    let err = client.workspace_rerun_setup(&workspace.id).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict), "already running");
}

/// A process running a command line containing this, or none.
fn pid_running(pattern: &str) -> Option<u32> {
    let out = Command::new("pgrep").args(["-f", pattern]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).split_whitespace().next().and_then(|p| p.parse().ok())
}

#[test]
fn a_setup_script_stops_with_its_hub_and_with_a_killed_worker() {
    let mut stack = Stack::start();
    // Unique to this test, so it can only find what this test started.
    let seconds = 7000 + std::process::id() % 1000;
    commit_settings(&stack, &format!("[scripts]\nsetup = \"sleep {seconds}\"\n"));
    let needle = format!("sleep {seconds}");
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_until("the script to start", || pid_running(&needle).is_some());

    // A clean stop, promptly, and not after the hub gives up waiting on its worker.
    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
    wait_until("the script to stop with it", || pid_running(&needle).is_none());
    drop(client);

    // A killed worker, which can't clean up after itself.
    stack.start_hub();
    let client = stack.client();
    assert_eq!(
        client.workspace_list(false).unwrap().into_iter().find(|w| w.id == workspace.id).unwrap().setup,
        protocol::model::SetupStatus::Failed { code: None },
        "it never finished"
    );
    client.workspace_rerun_setup(&workspace.id).unwrap();
    wait_until("the script to start again", || pid_running(&needle).is_some());
    let script = pid_running(&needle).unwrap();
    // SAFETY: kill(2) has no memory effects. This is the script's parent, the worker, by the chain below.
    let group_leader = parent_of(script);
    signal(parent_of(group_leader), libc::SIGKILL);
    wait_until("the script to be stopped", || pid_running(&needle).is_none());
}

#[test]
fn archiving_runs_the_archive_script_and_a_failure_blocks_unless_forced() {
    let stack = Stack::start();
    let marker = stack.path("archived-marker");
    commit_settings(&stack, &format!("[scripts]\narchive = \"touch {} && exit 1\"\n", marker.display()));
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);

    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(err.to_string().contains("archive script"), "{err}");
    assert!(marker.exists());
    assert!(PathBuf::from(workspace.path.clone().unwrap()).exists(), "a failed script leaves the workspace alone");

    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(!PathBuf::from(workspace.path.unwrap()).exists());
}

#[test]
fn restoring_runs_setup_again_because_what_it_made_went_with_the_worktree() {
    let stack = Stack::start();
    std::fs::write(stack.repo().join(".gitignore"), "made.txt\n").unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "ignore"]);
    commit_settings(&stack, "[scripts]\nsetup = \"touch made.txt\"\n");
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_for_setup(&client, &workspace.id);
    assert!(worktree.join("made.txt").exists());

    client.workspace_archive(&workspace.id, false).unwrap();
    assert!(!worktree.exists());
    client.workspace_restore(&workspace.id).unwrap();
    wait_until("setup to run again", || worktree.join("made.txt").exists());
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Succeeded);
}

#[test]
fn the_agent_sees_the_workspace_variables_and_each_workspace_has_its_own_ports() {
    let stack = Stack::start();
    let client = stack.client();
    let (first, session) = stack.workspace(&client);
    let (second, _) = stack.workspace(&client);
    assert_eq!(second.port_base, first.port_base + 10);
    client
        .session_prompt(
            &session.id,
            "env:CONDUCTOR_PORT; env:CONDUCTOR_SESSION_ID; env:CONDUCTOR_WORKSPACE_PATH; env:CONDUCTOR_IS_LOCAL",
            None,
        )
        .unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    assert!(text.contains(&format!("CONDUCTOR_PORT={}", first.port_base)), "{text}");
    assert!(text.contains(&format!("CONDUCTOR_SESSION_ID={}", session.id)), "{text}");
    assert!(text.contains(&format!("CONDUCTOR_WORKSPACE_PATH={}", first.path.unwrap())), "{text}");
    assert!(text.contains("CONDUCTOR_IS_LOCAL=1"), "{text}");
}

#[test]
fn workspace_names_are_checked_and_collisions_are_handled() {
    let stack = Stack::start();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    let err = client.workspace_create(&repo.id, Some("../escape".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));

    let first = client.workspace_create(&repo.id, Some("mine".into()), None).unwrap();
    let err = client.workspace_create(&repo.id, Some("mine".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));

    // A branch that exists outside the hub also blocks a chosen name, and leaves nothing behind.
    git(&stack.repo(), &["branch", "taken"]);
    let err = client.workspace_create(&repo.id, Some("taken".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert_eq!(client.workspace_list(true).unwrap()[0].id, first.id);

    let generated = client.workspace_create(&repo.id, None, None).unwrap();
    assert_ne!(generated.name, first.name);
}

#[test]
fn many_sessions_run_at_once_without_mixing_their_output() {
    let stack = Stack::start();
    let client = stack.client();
    let sessions: Vec<_> = (0..4)
        .map(|i| {
            let (workspace, session) = stack.workspace(&client);
            client.session_prompt(&session.id, &format!("slow:8:s{i}-; tool:pwd"), None).unwrap();
            (workspace, session, format!("s{i}-"))
        })
        .collect();
    for (_, session, marker) in &sessions {
        let events = wait_for_turn_end(&client, &session.id, 1);
        assert_gapless(&events);
        let text = assistant_text(&events);
        assert!(text.contains(marker.as_str()), "{text}");
        for (_, _, other) in &sessions {
            assert!(other == marker || !text.contains(other.as_str()), "{text}");
        }
    }
    assert_eq!(client.hub_status().unwrap().workers, 4);
}

// The connection ----------------------------------------------------------------------------------

fn next_notice(feed: &Subscription, what: &str) -> HubNotice {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match feed.items.recv_timeout(wait) {
            Ok(SubscriptionItem::Notice(notice)) => return notice,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for {what}: {e}"),
        }
    }
}

#[test]
fn the_feed_reports_what_a_sidebar_shows() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();

    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    assert_eq!(next_notice(&feed, "the repo"), HubNotice::Repo(repo.clone()));
    let workspace = client.workspace_create(&repo.id, Some("watched".into()), None).unwrap();
    // Ready, then its setup running and finishing.
    let HubNotice::Workspace(ready) = next_notice(&feed, "the workspace") else { panic!() };
    assert_eq!(ready.id, workspace.id);
    wait_for_setup(&client, &workspace.id);
    for _ in 0..2 {
        assert!(matches!(next_notice(&feed, "setup"), HubNotice::Workspace(_)));
    }
    let session =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into())).unwrap();
    assert_eq!(next_notice(&feed, "the session"), HubNotice::Session(session.clone()));

    // One notice per change of status, not one per event.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    let HubNotice::Session(running) = next_notice(&feed, "running") else { panic!() };
    assert_eq!((running.id.as_str(), running.status), (session.id.as_str(), SessionStatus::Running));
    let HubNotice::Session(idle) = next_notice(&feed, "idle") else { panic!() };
    assert_eq!(idle.status, SessionStatus::Idle);
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);

    let archived = client.workspace_archive(&workspace.id, true).unwrap();
    assert_eq!(next_notice(&feed, "the archive"), HubNotice::Workspace(archived));
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let HubNotice::Workspace(seen) = next_notice(&feed, "the restore") else { panic!() };
    assert_eq!((seen.id.as_str(), seen.status), (restored.id.as_str(), WorkspaceStatus::Ready));
}

fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn only_this_user_can_reach_the_hub_or_read_its_data() {
    let stack = Stack::start();
    assert_eq!(mode(&stack.path("home")), 0o700);
    assert_eq!(mode(&stack.socket()), 0o600);
    assert_eq!(mode(&stack.path("home/hub.db")), 0o600);
    // Created once there's something to write, and they must not be any looser.
    let client = stack.client();
    stack.workspace(&client);
    assert_eq!(mode(&stack.path("home/hub.db-wal")), 0o600);
}

#[test]
fn a_directory_the_user_pointed_us_at_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mut stack = Stack::start();
    stack.kill_hub();
    let shared = stack.path("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = stack.command();
    command.env("TRESTLE_HOME", &shared);
    let mut hub = command.spawn().unwrap();
    let socket = shared.join("hub.sock");
    wait_until("the hub", || Client::connect(&socket, "test").is_ok());
    assert_eq!(mode(&shared), 0o755);
    assert_eq!(mode(&socket), 0o600);
    assert_eq!(mode(&shared.join("hub.db")), 0o600);
    terminate(hub.id());
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
}

#[test]
fn dropping_a_client_really_disconnects_it() {
    let stack = Stack::start();
    let watcher = stack.client();
    let count = || watcher.hub_status().unwrap().clients;
    assert_eq!(count(), 1);
    let (_, session) = stack.workspace(&watcher);

    let other = stack.client();
    let _subscription = other.session_subscribe(&session.id, 1).unwrap();
    let clone = other.clone();
    assert_eq!(count(), 2);
    drop(other);
    assert_eq!(count(), 2, "a clone keeps it connected");
    drop(clone);
    wait_until("the hub to see the client leave", || count() == 1);
}

#[test]
fn dropping_a_subscription_stops_its_stream_but_not_the_connection() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let first = client.session_subscribe(&session.id, 1).unwrap();
    let id = first.id;
    drop(first);
    // The connection still works, and a new subscription gets everything from the start.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let second = client.session_subscribe(&session.id, 1).unwrap();
    assert_ne!(second.id, id);
    let mut follower = Follower::default();
    follower.drain_until(&second, "the whole log", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    assert_gapless(&follower.events);
}

#[test]
fn a_client_with_the_wrong_protocol_version_is_told_so() {
    use std::io::{BufRead, BufReader, Write};

    let stack = Stack::start();
    let mut stream = std::os::unix::net::UnixStream::connect(stack.socket()).unwrap();
    let hello = protocol::client::ClientFrame {
        id: 1,
        request: ClientRequest::Hello { protocol_version: 9999, client: "old".into() },
    };
    stream.write_all(&protocol::frame::encode(&hello).unwrap()).unwrap();
    let mut line = String::new();
    BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
    let reply: protocol::client::HubFrame = serde_json::from_str(&line).unwrap();
    match reply {
        protocol::client::HubFrame::Reply { outcome: protocol::client::ReplyOutcome::Error(e), .. } => {
            assert_eq!(e.code, ErrorCode::VersionMismatch);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_second_hub_refuses_to_start_and_leaves_the_first_alone() {
    let stack = Stack::start();
    let mut second = stack.command().spawn().unwrap();
    wait_until("the second hub to exit", || !matches!(second.try_wait(), Ok(None)));
    assert!(!second.wait().unwrap().success());
    assert_eq!(stack.client().hub_status().unwrap().pid, stack.hub_pid());
}

#[test]
fn losing_the_hub_ends_a_subscription_with_a_clear_signal() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    stack.kill_hub();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let item = subscription.items.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap();
        if item == SubscriptionItem::Disconnected {
            break;
        }
    }
    assert!(matches!(client.hub_status().unwrap_err(), ClientError::Disconnected | ClientError::Io(_)));
}

#[test]
fn the_agent_does_not_inherit_a_parent_sessions_identity() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    command.env("CLAUDECODE", "1").env("CLAUDE_CODE_SESSION_ID", "parent").env("ANTHROPIC_API_KEY", "kept");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client
        .session_prompt(
            &session.id,
            "env:CLAUDECODE; env:CLAUDE_CODE_SESSION_ID; env:ANTHROPIC_API_KEY; env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS",
            None,
        )
        .unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    assert!(text.contains("CLAUDECODE=<unset>"), "{text}");
    assert!(text.contains("CLAUDE_CODE_SESSION_ID=<unset>"), "{text}");
    assert!(text.contains("ANTHROPIC_API_KEY=kept"), "the user's own login settings stay: {text}");
    assert!(text.contains("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"), "{text}");
}

#[test]
fn raw_agent_lines_are_kept_so_the_log_can_be_rebuilt() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let (events, _) = client.session_read(&session.id, 1, 100, true).unwrap();
    let assistant = events.iter().find(|e| matches!(e.body, EventBody::Assistant { .. })).unwrap();
    let raw: serde_json::Value = serde_json::from_str(assistant.raw.as_ref().unwrap()).unwrap();
    assert_eq!(raw["message"]["content"][0]["text"], "hello");
    // The prompt was the hub's own record, so it has no raw line.
    assert!(events[0].raw.is_none());
}
