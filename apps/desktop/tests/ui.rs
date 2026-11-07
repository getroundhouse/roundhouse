//! The desktop app driven through its real UI, by clicks and typing, against a real hub, real workers
//! and real git worktrees. Only the agent is fake, and no window appears on screen. See
//! `apps/daemon/examples/fake_claude.rs` for what prompts can script.

#![cfg(test)]

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
use protocol::model::{SessionStatus, SetupStatus, WorkspaceStatus, seq_index};
use tempfile::TempDir;
use transcript::Transcript;

const TIMEOUT: Duration = Duration::from_secs(20);

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them.
fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new(env!("CARGO"))
            .args(["build", "-p", "daemon", "--bin", "trestled", "--example", "fake_claude"])
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
            std::env::set_var("TRESTLE_CLAUDE_BIN", binaries().join("examples/fake_claude"));
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
        let hub = HubSource { paths, daemon: binaries().join("trestled") };
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
    /// The approval [`Ui::next_open_approval`] last returned.
    seen_approval: u64,
}

impl<'a> Ui<'a> {
    fn open(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        // A real hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let hub = stack.hub.clone();
        let (handle, view) = cx.update(|cx| desktop::open_main_window(hub, cx)).unwrap();
        let mut ui = Self { cx, handle, view, seen_approval: 0 };
        ui.wait("the app to connect", |ui| matches!(ui.view().connection, Connection::Ready(_)));
        ui
    }

    fn view(&self) -> AppViewRef<'_> {
        AppViewRef(
            self.cx.read_entity(&self.view, |view, _| std::ptr::from_ref::<AppView>(view)),
            std::marker::PhantomData,
        )
    }

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket.
    /// The newest open approval, once the transcript has it. A session's status comes over the hub's
    /// feed, and the transcript's events over their own subscription, so either can arrive first, and
    /// the transcript can still be showing the previous approval.
    fn next_open_approval(&mut self) -> u64 {
        let seen = self.seen_approval;
        let newest = |ui: &Self| {
            let approvals = ui.transcript().map(|t| ui.cx.read_entity(&t, |t, _| t.open_approvals()));
            approvals.into_iter().flatten().map(|event| event.seq).filter(|seq| *seq > seen).max()
        };
        self.wait("the transcript to show the approval", |ui| newest(ui).is_some());
        self.seen_approval = newest(self).unwrap();
        self.seen_approval
    }

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
        self.pane().and_then(|pane| self.cx.read_entity(&pane, |p, _| p.transcript()))
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
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.status))
    }

    fn wait_idle_with(&mut self, what: &str, mut text: impl FnMut(&str) -> bool) {
        self.wait(what, |ui| ui.session_status() == Some(SessionStatus::Idle) && text(&ui.assistant_text()));
    }

    fn active_chat(&self) -> String {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.active.clone().unwrap())
    }

    fn open_chats(&self) -> Vec<String> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| {
            p.sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.id.clone()).collect()
        })
    }

    fn composer_text(&self) -> String {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, cx| p.composer.read(cx).text().to_string())
    }

    /// Presses the new chat button and waits until that chat is showing.
    fn new_chat(&mut self) -> String {
        let before = self.open_chats().len();
        self.click("new-chat");
        self.wait("the new chat", |ui| ui.open_chats().len() > before && ui.transcript().is_some());
        let newest = self.open_chats().last().unwrap().clone();
        self.wait("the new chat to show", |ui| ui.active_chat() == newest);
        newest
    }

    fn show_chat(&mut self, id: &str) {
        self.click(SharedString::from(format!("chat-{id}")));
        let id = id.to_string();
        self.wait("the chat to show", |ui| ui.active_chat() == id);
    }

    /// Opens a picker and clicks the item at this position, the way someone would.
    fn pick(&mut self, picker: &str, position: usize) {
        self.click(SharedString::from(picker.to_string()));
        self.click(ElementId::Integer(position as u64));
    }

    fn chat_mode(&self) -> Option<String> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.permission_mode.clone()))
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
    let approval_seq = ui.next_open_approval();
    ui.wait("the session to wait for an answer", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's approved");
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
    let total = seq_index(client.session_get(&session.id).unwrap().last_seq);
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
    let seq = ui.next_open_approval();
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
fn new_shows_its_working_until_the_workspace_is_ready_or_fails(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let repo = ui.first_repo_id();

    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.new_workspace(repo.clone(), cx));
    assert_eq!(ui.view().creating_in.as_ref(), Some(&repo));
    ui.wait("the workspace", |ui| ui.transcript().is_some() && ui.view().creating_in.is_none());

    // It can't get stuck showing that if creating one fails.
    std::fs::remove_dir_all(stack.repo()).unwrap();
    ui.cx.update_entity(&view, |v, cx| v.new_workspace(repo.clone(), cx));
    assert_eq!(ui.view().creating_in.as_ref(), Some(&repo));
    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().creating_in.is_none());
}

#[gpui_kit::test]
fn a_new_workspace_starts_with_one_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    // The first workspace is selected as soon as it exists, which is before its chat does.
    let workspace = ui.new_workspace();
    for _ in 0..30 {
        ui.frame();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ui.open_chats().len(), 1);
    assert_eq!(client.session_list(&workspace).unwrap().len(), 1);
}

#[gpui_kit::test]
fn chats_keep_their_own_transcript_and_draft(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    ui.input("say:from the first chat");
    ui.press("enter");
    ui.wait_idle_with("the first answer", |text| text.contains("from the first"));

    let second = ui.new_chat();
    assert_ne!(first, second);
    assert_eq!(ui.composer_text(), "");
    assert_eq!(ui.assistant_text(), "", "a new chat starts empty");
    ui.input("say:from the second chat");
    ui.press("enter");
    ui.wait_idle_with("the second answer", |text| text.contains("from the second"));
    assert!(!ui.assistant_text().contains("from the first"));

    // Half-typed text stays with its chat, and comes back with it.
    ui.input("half typed");
    ui.show_chat(&first);
    ui.frame();
    assert_eq!(ui.composer_text(), "", "the draft belongs to the other chat");
    assert!(ui.assistant_text().contains("from the first") && !ui.assistant_text().contains("from the second"));
    ui.show_chat(&second);
    ui.frame();
    assert_eq!(ui.composer_text(), "half typed");
    ui.input(" more");
    assert_eq!(ui.composer_text(), "half typed more", "the composer has focus after switching");

    // The first prompt names the chat, everywhere.
    let titles: Vec<_> = client.session_list(&ui.view().selected.clone().unwrap()).unwrap();
    assert_eq!(titles[0].title.as_deref(), Some("say:from the first chat"));
    ui.wait("the title to reach the app", |ui| {
        let pane = ui.pane().unwrap();
        ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.title.clone()))
            == Some("say:from the second chat".to_string())
    });
}

#[gpui_kit::test]
fn the_keyboard_opens_moves_between_and_closes_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    ui.press("cmd-t");
    ui.wait("a second chat", |ui| ui.open_chats().len() == 2);
    let second = ui.open_chats()[1].clone();
    ui.wait("it to show", |ui| ui.active_chat() == second);
    ui.press("ctrl-tab");
    ui.wait("the first chat, wrapping around", |ui| ui.active_chat() == first);
    ui.press("ctrl-shift-tab");
    ui.wait("the second chat, wrapping back", |ui| ui.active_chat() == second);
    ui.press("cmd-w");
    ui.wait("the second chat to close", |ui| ui.open_chats() == [first.clone()]);
    ui.wait("the first chat to show", |ui| ui.active_chat() == first);

    // Wherever focus was, switching chats puts it back in the composer, ready to type.
    ui.press("cmd-t");
    ui.wait("another chat", |ui| ui.open_chats().len() == 2);
    let third = ui.open_chats()[1].clone();
    ui.wait("it to show", |ui| ui.active_chat() == third);
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.press("ctrl-tab");
    ui.wait("the first chat", |ui| ui.active_chat() == first);
    ui.frame();
    ui.input("ready");
    assert_eq!(ui.composer_text(), "ready");
}

#[gpui_kit::test]
fn a_closed_chat_can_be_reopened_and_the_last_one_is_replaced(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let workspace = ui.view().selected.clone().unwrap();
    let first = ui.active_chat();
    ui.input("say:keep this");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("keep this"));

    // Closing the only chat leaves a new, empty one rather than nothing.
    ui.click(SharedString::from(format!("close-chat-{first}")));
    ui.wait("a replacement", |ui| ui.open_chats().len() == 1 && ui.open_chats()[0] != first);
    let replacement = ui.open_chats()[0].clone();
    ui.wait("it to show", |ui| ui.active_chat() == replacement && ui.transcript().is_some());
    assert_eq!(ui.assistant_text(), "");
    assert!(client.session_get(&first).unwrap().closed_at_ms.is_some());

    // The history is still there, and the agent picks up where it left off.
    ui.pick("closed-chats", 0);
    ui.wait("it to come back", |ui| ui.active_chat() == first && ui.assistant_text().contains("keep this"));
    assert_eq!(ui.open_chats().len(), 2);
    ui.input("recall");
    ui.press("enter");
    ui.wait_idle_with("the agent to remember", |text| text.contains("I have seen: say:keep this"));
    assert_eq!(client.session_list(&workspace).unwrap().len(), 2);
}

#[gpui_kit::test]
fn closing_a_chat_thats_working_asks_first(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let first = ui.active_chat();
    let working = ui.new_chat();
    ui.input("slow:200:still going");
    ui.press("enter");
    ui.wait("it to be working", |ui| ui.session_status() == Some(SessionStatus::Running));

    let close = format!("close-chat-{working}");
    ui.click(SharedString::from(close.clone()));
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Cancel");
    ui.frame();
    assert_eq!(ui.open_chats().len(), 2, "cancelling leaves it alone");
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));

    ui.click(SharedString::from(close));
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Close chat");
    ui.wait("it to close", |ui| ui.open_chats() == [first.clone()]);
    ui.wait("the other chat to show", |ui| ui.active_chat() == first);
}

#[gpui_kit::test]
fn the_pickers_change_a_running_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let chat = ui.active_chat();
    let path = ui.worktree(&workspace);

    // Ask first, chosen before anything has run, means the first launch asks.
    ui.pick("mode-picker", 0);
    ui.wait("the mode to change", |ui| ui.chat_mode().as_deref() == Some("manual"));
    ui.input("tool:echo one > one.txt");
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("allow".into(), seq));
    ui.wait("the first tool", |ui| ui.session_status() == Some(SessionStatus::Idle) && path.join("one.txt").exists());

    // Full access and another model, on the agent that's already running.
    ui.pick("mode-picker", 3);
    ui.wait("full access", |ui| ui.chat_mode().as_deref() == Some("bypassPermissions"));
    ui.pick("model-picker", 2);
    ui.wait("the model", |_| client.session_get(&chat).unwrap().model.as_deref() == Some("opus"));
    ui.input("tool:echo two > two.txt");
    ui.press("enter");
    ui.wait("the second tool, without asking", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && path.join("two.txt").exists()
    });
    let started = ui.events().iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the running agent was changed, not replaced");

    // A new chat doesn't quietly get full access.
    ui.new_chat();
    assert_eq!(ui.chat_mode(), None);
    let pane = ui.pane().unwrap();
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.model.clone())).as_deref(), Some("opus"));
}

#[gpui_kit::test]
fn a_change_the_agent_refuses_is_shown_and_changes_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:running");
    ui.press("enter");
    ui.wait_idle_with("the agent to be up", |text| text.contains("running"));
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, |p, cx| p.configure(Some("not-a-model".to_string()), None, cx));
    ui.wait("the refusal", |ui| ui.view().error.as_deref().is_some_and(|e| e.contains("not found")));
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.model.clone())), None);
}

#[gpui_kit::test]
fn a_plan_can_be_approved_or_sent_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.pick("mode-picker", 1);
    ui.wait("plan mode", |ui| ui.chat_mode().as_deref() == Some("plan"));

    ui.input("plan:First attempt");
    ui.press("enter");
    ui.wait("the plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("keep-planning".into(), seq));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert_eq!(ui.chat_mode().as_deref(), Some("plan"), "still planning");

    ui.input("plan:Second attempt");
    ui.press("enter");
    ui.wait("the next plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("approve-plan".into(), seq));
    ui.wait("plan mode to end, by the agent's own report", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && ui.chat_mode().as_deref() == Some("manual")
    });
}

/// The result of the agent's question, from what it was told.
fn told_to_agent(ui: &Ui) -> String {
    let mut told = String::new();
    for event in ui.events() {
        if let EventBody::AgentUser { blocks, .. } = &event.body {
            for block in blocks {
                if let protocol::event::ContentBlock::ToolResult { text, .. } = block {
                    told.push_str(text);
                }
            }
        }
    }
    told
}

fn open_question(ui: &mut Ui, prompt: &str) -> u64 {
    ui.input(prompt);
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval()
}

#[gpui_kit::test]
fn a_question_is_answered_by_clicking_or_typing_or_skipped(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();

    // One question with one answer needs only the click.
    let seq = open_question(&mut ui, "ask:Red or blue?=Red/Blue");
    assert!(
        ui.cx
            .update_window(ui.handle, |_, w, _| w.try_find(SharedString::from(format!("send-answers-{seq}"))).is_none())
            .unwrap()
    );
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    ui.wait("the answer", |ui| told_to_agent(ui).contains(r#""Red or blue?"=Blue"#));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Text typed while it's waiting answers it, rather than going to the agent as a new message.
    let seq = open_question(&mut ui, "ask:Which day?=Monday/Tuesday");
    ui.input("Friday, if that works");
    ui.press("enter");
    ui.wait("the typed answer", |ui| told_to_agent(ui).contains(r#""Which day?"=Friday, if that works"#));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert_eq!(ui.composer_text(), "");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 2, "the answer wasn't sent as a prompt too (seq {seq})");

    // Skipping lets the agent know, and once it's over, typing is a new message again.
    let seq = open_question(&mut ui, "ask:Tea or coffee?=Tea/Coffee");
    ui.click(SharedString::from(format!("skip-question-{seq}")));
    ui.wait("the skip", |ui| told_to_agent(ui).contains("chose not to answer"));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    ui.input("say:a new message");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("a new message"));
}

#[gpui_kit::test]
fn several_questions_wait_until_they_are_all_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();

    let seq = open_question(&mut ui, "ask:*Which colors?=Red/Green/Blue|How many?=One/Two");
    let send = format!("send-answers-{seq}");

    // Several can be chosen for the first, and choosing one again takes it back.
    ui.click(SharedString::from(format!("answer-{seq}-0-0")));
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    ui.click(SharedString::from(format!("answer-{seq}-0-2")));
    ui.click(SharedString::from(format!("answer-{seq}-0-1")));
    // Not answered yet, so pressing send does nothing.
    ui.click(SharedString::from(send.clone()));
    ui.frame();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval));
    assert_eq!(told_to_agent(&ui), "");

    // One answer allowed for the second, so a later choice replaces an earlier one.
    ui.click(SharedString::from(format!("answer-{seq}-1-0")));
    ui.click(SharedString::from(format!("answer-{seq}-1-1")));
    ui.click(SharedString::from(send));
    ui.wait("the answers", |ui| told_to_agent(ui).contains(r#""How many?"=Two"#));
    assert!(told_to_agent(&ui).contains(r#""Which colors?"=Red, Blue"#), "{}", told_to_agent(&ui));
}

#[gpui_kit::test]
fn theme_is_dark(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);
    assert!(ui.cx.update(|cx| Theme::global(cx).mode.is_dark()));
}
