//! The desktop app driven through its real UI, by clicks and typing, against a real hub, real workers
//! and real git worktrees. Only the agent is fake, and no window appears on screen. See
//! `apps/daemon/examples/fake_claude.rs` for what prompts can script.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use client_core::Client;
use desktop::app::{AppView, Connection, HubSource};
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::component::Theme;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, SharedString, TestAppContext};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{SessionStatus, SetupStatus, WorkspaceStatus};
use tempfile::TempDir;
use transcript::Transcript;

const TIMEOUT: Duration = Duration::from_secs(20);

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them.
fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new(env!("CARGO"))
            .args(["build", "-p", "daemon", "--bin", "agenthubd", "--example", "fake_claude"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        // target/debug/deps/ui-xxxx
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
    })
}

/// What the daemons these tests start need to run the fake agent. Set once, and never changed, because
/// tests here run in parallel and nothing else should be reading the environment at that moment.
fn point_daemons_at_the_fake_agent() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // Under Cargo's target directory, so runs can't pile up in the system's. Conversation IDs are
        // unique, so nothing here carries over from one run to the next.
        let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
        // SAFETY: guarded by the OnceLock, and this runs before any daemon or thread here reads it.
        unsafe {
            std::env::set_var("AGENTHUB_CLAUDE_BIN", binaries().join("examples/fake_claude"));
            std::env::set_var("FAKE_CLAUDE_STATE", scratch.join("ui-agent-state"));
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            std::env::set_var("HOME", scratch.join("ui-home"));
        }
    });
}

struct Stack {
    dir: TempDir,
    hub: HubSource,
}

impl Stack {
    fn new() -> Self {
        point_daemons_at_the_fake_agent();
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("ahui-").tempdir_in("/tmp").unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("repo")).unwrap();
        git(&root.join("repo"), &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("repo/a.txt"), "one\n").unwrap();
        git(&root.join("repo"), &["add", "."]);
        git(&root.join("repo"), &["commit", "-qm", "first"]);
        let paths = protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME => Some(root.join("home").to_string_lossy().into_owned()),
            protocol::paths::ENV_WORKSPACES => Some(root.join("workspaces").to_string_lossy().into_owned()),
            other => std::env::var(other).ok(),
        })
        .unwrap();
        let hub = HubSource { paths, daemon: binaries().join("agenthubd") };
        Self { dir, hub }
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    /// A second client, for setting things up and checking them from outside the app.
    fn client(&self) -> Client {
        Client::connect_or_start(&self.hub.paths, &self.hub.daemon, "test").unwrap()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Ok(client) = Client::connect(&self.hub.paths.socket, "test") {
            let _ = client.hub_shutdown();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.hub.paths.socket.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
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

struct Ui<'a> {
    cx: &'a mut TestAppContext,
    handle: AnyWindowHandle,
    view: Entity<AppView>,
}

impl<'a> Ui<'a> {
    fn open(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        // A real hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let hub = stack.hub.clone();
        let (handle, view) = cx.update(|cx| desktop::open_main_window(hub, cx)).unwrap();
        let mut ui = Self { cx, handle, view };
        ui.wait("the app to connect", |ui| matches!(ui.view().connection, Connection::Ready(_)));
        ui
    }

    fn view(&self) -> AppViewRef<'_> {
        AppViewRef(self.cx.read_entity(&self.view, |view, _| view as *const AppView), std::marker::PhantomData)
    }

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket.
    fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn frame(&mut self) {
        self.cx.run_until_parked();
        let _ = self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx));
        self.cx.run_until_parked();
    }

    fn click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
        self.frame();
    }

    fn input(&mut self, text: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
    }

    fn press(&mut self, key: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.press(key, cx)).unwrap();
        self.frame();
    }

    fn pane(&self) -> Option<Entity<WorkspacePane>> {
        let view = self.view();
        view.selected.as_ref().and_then(|id| view.panes.get(id).cloned())
    }

    fn transcript(&self) -> Option<Entity<Transcript>> {
        self.pane().and_then(|pane| self.cx.read_entity(&pane, |p, _| p.transcript.clone()))
    }

    fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.transcript() else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.event_at(row)).collect())
    }

    fn assistant_text(&self) -> String {
        let mut text = String::new();
        for event in self.events() {
            if let EventBody::Assistant { blocks, .. } = &event.body {
                for block in blocks {
                    if let protocol::event::ContentBlock::Text { text: t } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    fn session_status(&self) -> Option<SessionStatus> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session.as_ref().map(|s| s.status))
    }

    fn wait_idle_with(&mut self, what: &str, mut text: impl FnMut(&str) -> bool) {
        self.wait(what, |ui| ui.session_status() == Some(SessionStatus::Idle) && text(&ui.assistant_text()));
    }

    fn first_repo_id(&self) -> String {
        self.view().repos[0].id.clone()
    }

    /// Clicks New for the first repository, and waits until the new workspace is on screen.
    fn new_workspace(&mut self) -> String {
        let before = self.view().workspaces.len();
        let id = format!("new-in-{}", self.first_repo_id());
        self.click(SharedString::from(id));
        self.wait("the new workspace", |ui| {
            ui.view().workspaces.len() > before && ui.transcript().is_some() && !ui.view().busy
        });
        self.view().selected.clone().unwrap()
    }

    fn worktree(&self, id: &str) -> PathBuf {
        let view = self.view();
        let workspace = view.workspaces.iter().find(|w| w.id == id).unwrap();
        PathBuf::from(workspace.path.clone().unwrap())
    }
}

/// Closes the window, so the app isn't there to start a new hub when the test stops this one. A real
/// window does that on purpose, and in a test it would leave a hub running after the test has finished.
impl Drop for Ui<'_> {
    fn drop(&mut self) {
        let _ = self.cx.update_window(self.handle, |_, window, _| window.remove_window());
        self.cx.run_until_parked();
    }
}

/// Reads the view without holding a borrow of the test context across a wait.
struct AppViewRef<'a>(*const AppView, std::marker::PhantomData<&'a ()>);

impl std::ops::Deref for AppViewRef<'_> {
    type Target = AppView;
    fn deref(&self) -> &AppView {
        // SAFETY: the view lives as long as the window, for the whole test, and everything here runs on
        // one thread. These borrows only last for a single read, never across a frame.
        unsafe { &*self.0 }
    }
}

#[gpui_kit::test]
fn starts_empty_and_shows_a_repository_added_from_elsewhere(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.view().repos.is_empty());
    assert!(ui.pane().is_none());

    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository to appear", |ui| ui.view().repos.len() == 1);
    assert_eq!(ui.view().repos[0].name, "repo");
}

#[gpui_kit::test]
fn the_core_loop(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);

    // Create a workspace. The composer is ready to type in straight away.
    let id = ui.new_workspace();
    let worktree = ui.worktree(&id);
    assert!(worktree.join("a.txt").exists());
    let branch = git(&worktree, &["branch", "--show-current"]);

    // Work with the agent, approving a tool on the way.
    ui.input("say:starting; tool:echo changed >> a.txt; say:finished");
    ui.press("enter");
    ui.wait("the agent to ask for approval", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| !t.open_approvals().is_empty()))
    });
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's approved");
    let approval_seq = ui.transcript().map(|t| ui.cx.read_entity(&t, |t, _| t.open_approvals()[0].seq)).unwrap();
    ui.click(ElementId::NamedInteger("allow".into(), approval_seq));
    ui.wait_idle_with("the turn to finish", |text| text.contains("finished"));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");

    // Review the diff.
    ui.click("toggle-diff");
    let pane = ui.pane().unwrap();
    ui.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
    let diff = ui.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
    ui.wait("the diff to load", |ui| ui.cx.read_entity(&diff, |d, _| d.state.is_some()));

    // Comment on a line. Enter saves it, and the hub keeps it.
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            diff.update(cx, |d, cx| {
                d.start_comment(DiffLinePosition::new("a.txt", DiffSide::Modified, 2), window, cx);
            });
        })
        .unwrap();
    ui.frame();
    ui.input("Why this word?");
    ui.press("enter");
    ui.wait("the comment to be saved", |ui| ui.cx.read_entity(&diff, |d, _| d.comments.len() == 1));
    assert_eq!(stack.client().comment_list(&id).unwrap().len(), 1);

    // Send it to the agent, which sees where it was left.
    ui.click("send-comments");
    ui.wait("the comment to reach the agent", |ui| {
        ui.events().iter().any(
            |e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("`a.txt:2`: Why this word?")),
        )
    });
    ui.wait("the comment to be cleared", |ui| {
        ui.cx.read_entity(&diff, |d, _| d.comments.is_empty()) && stack.client().comment_list(&id).unwrap().is_empty()
    });
    ui.wait("the turn to finish", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Open a PR, which is the agent's job.
    ui.click("create-pr");
    ui.wait("the PR request to reach the agent", |ui| {
        ui.events()
            .iter()
            .any(|e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("gh pr create")))
    });
    ui.wait("the turn to finish", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Archive. Uncommitted work means asking first, and "keep working" leaves it all alone.
    ui.click("archive");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();
    assert!(worktree.join("a.txt").exists());
    assert_eq!(ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status, WorkspaceStatus::Ready);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    ui.click("archive");
    ui.wait("the workspace to be archived", |ui| {
        ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status == WorkspaceStatus::Archived && !ui.view().busy
    });
    assert!(!worktree.exists());
    assert!(ui.pane().is_none());
    assert!(!git(&stack.repo(), &["branch", "--list", &branch]).is_empty(), "the branch is kept");

    // Restoring brings the worktree and the history back, and the conversation continues.
    ui.click("toggle-archived");
    ui.cx.update_entity(&ui.view.clone(), |v, cx| v.restore(id.clone(), cx));
    ui.wait("the workspace to come back", |ui| ui.transcript().is_some() && !ui.view().busy);
    ui.wait("the history to load", |ui| !ui.events().is_empty());
    assert!(worktree.join("a.txt").exists());
    ui.input("recall");
    ui.press("enter");
    ui.wait_idle_with("the resumed agent to answer", |text| text.contains("I have seen:"));
    assert!(ui.assistant_text().contains("say:starting"), "{}", ui.assistant_text());
}

#[gpui_kit::test]
fn clicking_a_workspace_switches_to_it_and_typing_goes_to_the_right_place(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let first = ui.new_workspace();
    let second = ui.new_workspace();
    assert_ne!(first, second);
    assert_eq!(ui.view().selected.as_ref(), Some(&second));

    ui.click(SharedString::from(format!("workspace-{first}")));
    ui.wait("the first workspace", |ui| ui.view().selected.as_ref() == Some(&first) && ui.pane().is_some());
    ui.input("say:in the first");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("in the first"));

    ui.click(SharedString::from(format!("workspace-{second}")));
    ui.wait("the second workspace", |ui| ui.view().selected.as_ref() == Some(&second));
    ui.frame();
    assert!(!ui.assistant_text().contains("in the first"), "the second workspace has its own transcript");
    ui.input("say:in the second");
    ui.click("send");
    ui.wait_idle_with("the answer", |text| text.contains("in the second"));
    assert!(!ui.assistant_text().contains("in the first"));
}

#[gpui_kit::test]
fn the_keyboard_moves_between_workspaces_and_opens_the_diff(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let first = ui.new_workspace();
    let second = ui.new_workspace();

    ui.press("cmd-[");
    ui.wait("the previous workspace", |ui| ui.view().selected.as_ref() == Some(&first));
    ui.press("cmd-]");
    ui.wait("the next workspace", |ui| ui.view().selected.as_ref() == Some(&second));
    ui.press("cmd-]");
    ui.wait("wrapping around", |ui| ui.view().selected.as_ref() == Some(&first));

    ui.wait("the pane", |ui| ui.pane().is_some());
    ui.press("cmd-shift-d");
    let pane = ui.pane().unwrap();
    ui.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff_open && p.diff.is_some()));
    let diff = ui.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
    ui.wait("it to see there are no changes", |ui| ui.cx.read_entity(&diff, |d, _| d.empty));

    let before = ui.view().workspaces.len();
    ui.press("cmd-n");
    ui.wait("a third workspace", |ui| ui.view().workspaces.len() == before + 1 && !ui.view().busy);
}

fn commit_settings(stack: &Stack, toml: &str) {
    let dir = stack.repo().join(".conductor");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.toml"), toml).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "settings"]);
}

#[gpui_kit::test]
fn a_failed_setup_is_shown_with_its_log_and_can_be_run_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Fails until the file it's looking for exists.
    let flag = stack.dir.path().join("may-succeed");
    commit_settings(&stack, &format!("[scripts]\nsetup = \"echo from-the-script; test -f {}\"\n", flag.display()));
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();
    let setup = |ui: &Ui| ui.cx.read_entity(&pane, |p, _| p.workspace.setup);
    ui.wait("setup to fail", |ui| matches!(setup(ui), SetupStatus::Failed { .. }));

    ui.click("setup-log");
    ui.wait("the log", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.setup_log.as_deref().is_some_and(|log| log.contains("from-the-script")))
    });
    ui.click("setup-log");
    assert!(ui.cx.read_entity(&pane, |p, _| p.setup_log.is_none()));

    std::fs::write(&flag, "").unwrap();
    ui.click("rerun-setup");
    ui.wait("setup to succeed", |ui| setup(ui) == SetupStatus::Succeeded);
    // Nothing more to say, so the line and its buttons go.
    ui.frame();
    let gone = ui.cx.update_window(ui.handle, |_, window, _| window.try_find("rerun-setup").is_none()).unwrap();
    assert!(gone);
}

#[gpui_kit::test]
fn setup_can_be_stopped_from_the_app(cx: &mut TestAppContext) {
    let stack = Stack::new();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 300\"\n");
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();
    ui.wait("setup to start", |ui| ui.cx.read_entity(&pane, |p, _| p.workspace.setup == SetupStatus::Running));
    ui.click("stop-setup");
    ui.wait("setup to stop", |ui| {
        ui.cx.read_entity(&pane, |p, _| matches!(p.workspace.setup, SetupStatus::Failed { code: None }))
    });
}

#[gpui_kit::test]
fn text_streams_in_as_it_is_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("slow:20:word");
    ui.press("enter");
    ui.wait("streaming text", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("word5")))
    });
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    ui.wait_idle_with("the finished message", |text| text.contains("word19"));
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().is_empty())));
}

#[gpui_kit::test]
fn the_stop_button_stops_the_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:go; hang:60000");
    ui.press("enter");
    ui.wait("the agent to start", |ui| ui.assistant_text().contains("go"));
    ui.wait("the Stop button", |ui| {
        ui.session_status() == Some(SessionStatus::WaitingApproval)
            || ui.session_status() == Some(SessionStatus::Running)
    });
    ui.click("stop");
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert!(
        ui.events()
            .iter()
            .any(|e| matches!(e.body, EventBody::TurnEnded { outcome: protocol::event::TurnOutcome::Cancelled, .. }))
    );
}

#[gpui_kit::test]
fn long_history_loads_a_page_at_a_time_as_it_is_scrolled_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None).unwrap();
    let session = client
        .session_create(&workspace.id, protocol::model::AgentKind::Claude, None, Some("bypassPermissions".into()))
        .unwrap();
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    client.session_prompt(&session.id, &script.join("; "), None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session_get(&session.id).unwrap().status != SessionStatus::Idle {
        assert!(Instant::now() < deadline, "the agent never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
    let total = client.session_get(&session.id).unwrap().last_seq as usize;
    assert!(total > 700);

    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    // Only the tail so far. The start of the session isn't held.
    let loaded = ui.cx.read_entity(&transcript, |t, _| t.loaded_events());
    assert!(loaded < total, "{loaded} of {total}");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()));

    let scroller = ui.cx.read_entity(&transcript, |t, _| t.scroller().clone());
    ui.cx.update_entity(&scroller, |list, cx| {
        list.scroll_to_item(0, cx);
    });
    ui.wait("the first page to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_some()));
    assert!(matches!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).unwrap().body), EventBody::UserPrompt { .. }));
}

#[gpui_kit::test]
fn losing_the_hub_starts_a_new_one_and_picks_up_where_it_left_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:before");
    ui.press("enter");
    ui.wait_idle_with("the first turn", |text| text.contains("before"));

    let pid = stack.client().hub_status().unwrap().pid;
    let killed = Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap();
    assert!(killed.success());

    ui.wait("a new hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_))
            && stack.client().hub_status().is_ok_and(|status| status.pid != pid)
            && ui.transcript().is_some()
    });
    ui.wait("the history", |ui| ui.assistant_text().contains("before"));
    ui.input("say:after");
    ui.press("enter");
    ui.wait_idle_with("the next turn", |text| text.contains("after"));
}

#[gpui_kit::test]
fn a_failure_from_the_hub_is_shown_rather_than_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let repo = stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    // The repository disappears, so a new worktree can't be made from it.
    std::fs::remove_dir_all(stack.repo()).unwrap();
    let id = format!("new-in-{}", repo.id);
    ui.click(SharedString::from(id));
    ui.wait("the error", |ui| ui.view().error.is_some());
    assert!(ui.view().error.clone().unwrap().contains("couldn't create the workspace"));
    assert!(!ui.view().busy, "the button works again");
}

#[gpui_kit::test]
fn empty_prompts_are_not_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.press("enter");
    ui.input("   ");
    ui.press("enter");
    ui.frame();
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.row_count() == 0)));
    // They never reach the hub, so there's nothing to complain about either.
    assert_eq!(ui.view().error, None);
}

#[gpui_kit::test]
fn a_sent_prompt_shows_once_and_becomes_the_recorded_event(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hello"));
    let transcript = ui.transcript().unwrap();
    let rows = ui.cx.read_entity(&transcript, |t, _| t.row_count());
    let recorded = ui.events().len();
    assert_eq!(rows, recorded, "nothing is left waiting to be recorded");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
}

#[gpui_kit::test]
fn an_approval_waiting_when_the_app_opens_can_be_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None).unwrap();
    let session =
        client.session_create(&workspace.id, protocol::model::AgentKind::Claude, None, Some("manual".into())).unwrap();
    client.session_prompt(&session.id, "tool:echo answered > answered.txt", None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session_get(&session.id).unwrap().status != SessionStatus::WaitingApproval {
        assert!(Instant::now() < deadline, "the agent never asked");
        std::thread::sleep(Duration::from_millis(20));
    }

    // The app opens after the question was asked, so it has to learn about it from history.
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    ui.wait("the waiting approval", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| !t.open_approvals().is_empty()))
    });
    let seq = ui.transcript().map(|t| ui.cx.read_entity(&t, |t, _| t.open_approvals()[0].seq)).unwrap();
    ui.click(ElementId::NamedInteger("allow".into(), seq));
    ui.wait("the tool to run", |ui| {
        ui.session_status() == Some(SessionStatus::Idle)
            && PathBuf::from(workspace.path.clone().unwrap()).join("answered.txt").exists()
    });
}

#[gpui_kit::test]
fn losing_the_hub_with_nothing_open_still_reconnects(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.pane().is_none());
    let pid = stack.client().hub_status().unwrap().pid;
    assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());
    ui.wait("a new hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_))
            && Client::connect(&stack.hub.paths.socket, "test")
                .is_ok_and(|c| c.hub_status().is_ok_and(|s| s.pid != pid))
    });
    // And it's really connected: something added elsewhere still shows up.
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
}

#[gpui_kit::test]
fn theme_is_dark(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);
    assert!(ui.cx.update(|cx| Theme::global(cx).mode.is_dark()));
}
