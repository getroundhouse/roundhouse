//! The window: the sidebar on the left and the selected workspace beside it. Holds the hub's records,
//! kept current by its feed, and is the only place that connects and reconnects.

use std::collections::HashMap;
use std::path::PathBuf;

use client_core::{Client, ClientError, SubscriptionItem};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{AgentKind, Repo, RepoId, Session, Workspace, WorkspaceId, WorkspaceStatus};
use protocol::paths::Paths;

use crate::workspace_pane::{PaneEvent, WorkspacePane};
use crate::{
    AddRepository, CloseChat, CreatePullRequest, NewChat, NewWorkspace, NextChat, NextWorkspace, PreviousChat,
    PreviousWorkspace, StopAgent, ToggleDiff,
};

/// Where to find the hub, and how to start one if none is running.
#[derive(Clone, Debug)]
pub struct HubSource {
    pub paths: Paths,
    pub daemon: PathBuf,
}

impl HubSource {
    /// The default hub for this user, with the daemon next to this binary.
    pub fn default_local() -> Self {
        let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).expect("HOME is set");
        let daemon = std::env::var_os("AGENTHUB_DAEMON_BIN").map(PathBuf::from).unwrap_or_else(|| {
            std::env::current_exe().map(|exe| exe.with_file_name("agenthubd")).unwrap_or_else(|_| "agenthubd".into())
        });
        Self { paths, daemon }
    }

    fn connect(&self) -> Result<Client, ClientError> {
        Client::connect_or_start(&self.paths, &self.daemon, "desktop")
    }
}

pub enum Connection {
    Connecting,
    Ready(Client),
    Failed(String),
}

struct Snapshot {
    repos: Vec<Repo>,
    workspaces: Vec<Workspace>,
    sessions: HashMap<WorkspaceId, Vec<Session>>,
}

fn load_snapshot(client: &Client) -> Result<Snapshot, ClientError> {
    let repos = client.repo_list()?;
    let workspaces = client.workspace_list(true)?;
    let mut sessions = HashMap::new();
    for workspace in workspaces.iter().filter(|w| w.status == WorkspaceStatus::Ready) {
        sessions.insert(workspace.id.clone(), client.session_list(&workspace.id)?);
    }
    Ok(Snapshot { repos, workspaces, sessions })
}

pub struct AppView {
    hub: HubSource,
    pub connection: Connection,
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: HashMap<WorkspaceId, Vec<Session>>,
    pub selected: Option<WorkspaceId>,
    pub panes: HashMap<WorkspaceId, Entity<WorkspacePane>>,
    pub error: Option<String>,
    pub show_archived: bool,
    /// Workspaces being created or archived, so their buttons can't be pressed twice.
    pub busy: bool,
    focus: FocusHandle,
    window: AnyWindowHandle,
    _feed: Option<Task<()>>,
}

impl AppView {
    pub fn new(hub: HubSource, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            hub,
            connection: Connection::Connecting,
            repos: Vec::new(),
            workspaces: Vec::new(),
            sessions: HashMap::new(),
            selected: None,
            panes: HashMap::new(),
            error: None,
            show_archived: false,
            busy: false,
            focus,
            window: window.window_handle(),
            _feed: None,
        };
        this.connect(cx);
        this
    }

    pub fn client(&self) -> Option<Client> {
        match &self.connection {
            Connection::Ready(client) => Some(client.clone()),
            _ => None,
        }
    }

    pub fn show_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// Connects and follows the feed. Comes back here if the hub goes away, which starts a new one.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        self.panes.clear();
        cx.notify();
        let hub = self.hub.clone();
        self._feed = Some(cx.spawn(async move |this, cx| {
            let connected = cx
                .background_spawn(async move {
                    let client = hub.connect()?;
                    // Subscribing first means nothing can change between the list and the feed.
                    let feed = client.feed_subscribe()?;
                    let snapshot = load_snapshot(&client)?;
                    Ok::<_, ClientError>((client, feed, snapshot))
                })
                .await;
            let (client, feed, snapshot) = match connected {
                Ok(connected) => connected,
                Err(e) => {
                    let _ = this.update(cx, |t, cx| {
                        t.connection = Connection::Failed(format!("{e}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |t, cx| {
                t.connection = Connection::Ready(client);
                t.apply_snapshot(snapshot, cx);
            });
            while let Ok(item) = feed.items.recv_async().await {
                match item {
                    SubscriptionItem::Notice(notice) => {
                        if this.update(cx, |t, cx| t.apply_notice(notice, cx)).is_err() {
                            return;
                        }
                    }
                    SubscriptionItem::Disconnected => {
                        let _ = this.update(cx, |t, cx| t.connect(cx));
                        return;
                    }
                    _ => {}
                }
            }
        }));
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.repos = snapshot.repos;
        self.workspaces = snapshot.workspaces;
        self.sessions = snapshot.sessions;
        for (id, pane) in &self.panes {
            let sessions = self.sessions.get(id).cloned().unwrap_or_default();
            pane.update(cx, |p, cx| p.set_sessions(sessions, cx));
        }
        let still_there =
            self.selected.as_ref().is_some_and(|id| self.visible_workspaces().iter().any(|w| &w.id == id));
        if !still_there {
            self.selected = self.visible_workspaces().first().map(|w| w.id.clone());
        }
        self.show_selected(cx);
        cx.notify();
    }

    fn apply_notice(&mut self, notice: HubNotice, cx: &mut Context<Self>) {
        match notice {
            HubNotice::Repo(repo) => match self.repos.iter_mut().find(|r| r.id == repo.id) {
                Some(existing) => *existing = repo,
                None => self.repos.push(repo),
            },
            HubNotice::Workspace(workspace) => {
                if workspace.status == WorkspaceStatus::Archived {
                    self.panes.remove(&workspace.id);
                }
                if let Some(pane) = self.panes.get(&workspace.id) {
                    pane.update(cx, |p, cx| p.set_workspace(workspace.clone(), cx));
                }
                match self.workspaces.iter_mut().find(|w| w.id == workspace.id) {
                    Some(existing) => *existing = workspace,
                    None => self.workspaces.push(workspace),
                }
                self.keep_selection_valid(cx);
            }
            HubNotice::Session(session) => {
                if let Some(pane) = self.panes.get(&session.workspace_id) {
                    pane.update(cx, |p, cx| p.set_session(session.clone(), cx));
                }
                let list = self.sessions.entry(session.workspace_id.clone()).or_default();
                match list.iter_mut().find(|s| s.id == session.id) {
                    Some(existing) => *existing = session,
                    None => list.push(session),
                }
            }
            HubNotice::Resync => {
                if let Some(client) = self.client() {
                    cx.spawn(async move |this, cx| {
                        let snapshot = cx.background_spawn(async move { load_snapshot(&client) }).await;
                        let _ = this.update(cx, |t, cx| match snapshot {
                            Ok(snapshot) => t.apply_snapshot(snapshot, cx),
                            Err(e) => t.show_error(format!("{e}"), cx),
                        });
                    })
                    .detach();
                }
            }
            HubNotice::Unknown => {}
        }
        cx.notify();
    }

    fn keep_selection_valid(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible_workspaces();
        let fine = self.selected.as_ref().is_some_and(|id| visible.iter().any(|w| &w.id == id));
        if !fine {
            self.selected = visible.first().map(|w| w.id.clone());
            self.show_selected(cx);
        }
    }

    pub fn visible_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().filter(|w| self.show_archived || w.status == WorkspaceStatus::Ready).collect()
    }

    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.show_selected(cx);
        cx.notify();
    }

    fn step_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let visible: Vec<WorkspaceId> = self.visible_workspaces().iter().map(|w| w.id.clone()).collect();
        if visible.is_empty() {
            return;
        }
        let at = self.selected.as_ref().and_then(|id| visible.iter().position(|v| v == id)).unwrap_or(0) as isize;
        let next = (at + by).rem_euclid(visible.len() as isize) as usize;
        self.select(visible[next].clone(), cx);
    }

    /// Makes sure the selected workspace is on screen with its composer focused. This needs the window,
    /// and a click handler is already inside a window update, so it runs just after whatever called it.
    fn show_selected(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| t.make_selected_pane(window, cx));
            });
        });
    }

    fn make_selected_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        if !self.panes.contains_key(&id) {
            let (Some(client), Some(workspace)) = (self.client(), self.workspaces.iter().find(|w| w.id == id).cloned())
            else {
                return;
            };
            if workspace.status != WorkspaceStatus::Ready {
                return;
            }
            let sessions = self.sessions.get(&id).cloned().unwrap_or_default();
            let pane = cx.new(|cx| WorkspacePane::new(client, workspace, sessions, window, cx));
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Disconnected => t.connect(cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
            })
            .detach();
            self.panes.insert(id.clone(), pane);
        }
        if let Some(pane) = self.panes.get(&id) {
            pane.update(cx, |p, cx| p.focus_composer(window, cx));
        }
        cx.notify();
    }

    pub fn new_workspace(&mut self, repo_id: RepoId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if self.busy {
            return;
        }
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let created = cx
                .background_spawn(async move {
                    let workspace = client.workspace_create(&repo_id, None, None)?;
                    // Leaves the model and approval mode to the user's own Claude settings.
                    let session = client.session_ensure(&workspace.id, AgentKind::Claude)?;
                    Ok::<_, ClientError>((workspace, session))
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
                match created {
                    Ok((workspace, session)) => {
                        let id = workspace.id.clone();
                        if !t.workspaces.iter().any(|w| w.id == id) {
                            t.workspaces.push(workspace);
                        }
                        let list = t.sessions.entry(id.clone()).or_default();
                        if !list.iter().any(|s| s.id == session.id) {
                            list.push(session);
                        }
                        t.select(id, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't create the workspace: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn add_repository(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let added = cx.background_spawn(async move { client.repo_add(&path.to_string_lossy()) }).await;
            let _ = this.update(cx, |t, cx| match added {
                Ok(repo) => {
                    if !t.repos.iter().any(|r| r.id == repo.id) {
                        t.repos.push(repo);
                    }
                    cx.notify();
                }
                Err(e) => t.show_error(format!("couldn't add that: {e}"), cx),
            });
        })
        .detach();
    }

    /// Archives, and asks before losing uncommitted work.
    pub fn archive(&mut self, id: WorkspaceId, force: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let target = id.clone();
            let result = cx.background_spawn(async move { client.workspace_archive(&target, force) }).await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
                match result {
                    Ok(workspace) => {
                        t.panes.remove(&workspace.id);
                        if let Some(existing) = t.workspaces.iter_mut().find(|w| w.id == workspace.id) {
                            *existing = workspace;
                        }
                        t.keep_selection_valid(cx);
                    }
                    Err(ClientError::Api(e)) if e.code == ErrorCode::Conflict && !force => {
                        t.confirm_forced_archive(id.clone(), e.message, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't archive: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn confirm_forced_archive(&mut self, id: WorkspaceId, reason: String, cx: &mut Context<Self>) {
        let handle = self.window;
        let answer = handle.update(cx, |_, window, cx| {
            window.prompt(
                PromptLevel::Warning,
                "Archive anyway?",
                Some(&reason),
                &["Keep working", "Archive anyway"],
                cx,
            )
        });
        let Ok(answer) = answer else { return };
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |t, cx| t.archive(id, true, cx));
            }
        })
        .detach();
    }

    pub fn restore(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let workspace = client.workspace_restore(&id)?;
                    let sessions = client.session_list(&workspace.id)?;
                    Ok::<_, ClientError>((workspace, sessions))
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
                match result {
                    Ok((workspace, sessions)) => {
                        let id = workspace.id.clone();
                        t.sessions.insert(id.clone(), sessions);
                        match t.workspaces.iter_mut().find(|w| w.id == id) {
                            Some(existing) => *existing = workspace,
                            None => t.workspaces.push(workspace),
                        }
                        t.select(id, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't restore: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn selected_pane(&self) -> Option<Entity<WorkspacePane>> {
        self.selected.as_ref().and_then(|id| self.panes.get(id).cloned())
    }

    /// The repository to put a new workspace in: the selected workspace's, or the first.
    pub fn current_repo(&self) -> Option<RepoId> {
        let selected = self.selected.as_ref().and_then(|id| self.workspaces.iter().find(|w| &w.id == id));
        selected.map(|w| w.repo_id.clone()).or_else(|| self.repos.first().map(|r| r.id.clone()))
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Shortcuts only reach this view through whatever has focus, and focus can end up nowhere, as
        // when what had it goes away. Every shortcut would then do nothing until something is clicked.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let theme = cx.theme();
        let (background, foreground, muted, danger) =
            (theme.background, theme.foreground, theme.muted_foreground, theme.danger);
        let main: AnyElement = match &self.connection {
            Connection::Connecting => centered("Connecting…", muted),
            Connection::Failed(reason) => centered(&format!("Couldn't reach the hub. {reason}"), danger),
            Connection::Ready(_) => match self.selected_pane() {
                Some(pane) => pane.into_any_element(),
                None if self.repos.is_empty() => centered("Add a repository to get started.", muted),
                None => centered("Create a workspace to get started.", muted),
            },
        };
        h_flex()
            .id("app")
            .track_focus(&self.focus)
            .size_full()
            .bg(background)
            .text_color(foreground)
            .on_action(cx.listener(|t, _: &NewWorkspace, _, cx| {
                if let Some(repo) = t.current_repo() {
                    t.new_workspace(repo, cx);
                }
            }))
            .on_action(cx.listener(|t, _: &AddRepository, _, cx| t.add_repository(cx)))
            .on_action(cx.listener(|t, _: &NextWorkspace, _, cx| t.step_selection(1, cx)))
            .on_action(cx.listener(|t, _: &PreviousWorkspace, _, cx| t.step_selection(-1, cx)))
            .on_action(cx.listener(|t, _: &ToggleDiff, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.toggle_diff(cx));
                }
            }))
            .on_action(cx.listener(|t, _: &CreatePullRequest, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.create_pull_request(cx));
                }
            }))
            .on_action(cx.listener(|t, _: &NewChat, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.new_chat(cx));
                }
            }))
            .on_action(cx.listener(|t, _: &CloseChat, window, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| {
                        if let Some(id) = p.active.clone() {
                            p.close_chat(id, window, cx);
                        }
                    });
                }
            }))
            .on_action(cx.listener(|t, _: &NextChat, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.step_chat(1, cx));
                }
            }))
            .on_action(cx.listener(|t, _: &PreviousChat, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.step_chat(-1, cx));
                }
            }))
            .on_action(cx.listener(|t, _: &StopAgent, _, cx| {
                if let Some(pane) = t.selected_pane() {
                    pane.update(cx, |p, cx| p.stop_agent(cx));
                }
            }))
            .child(crate::sidebar::render(self, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when_some(self.error.clone(), |d, error| {
                        d.child(
                            h_flex()
                                .id("error-bar")
                                .flex_none()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .bg(danger.opacity(0.15))
                                .text_sm()
                                .text_color(danger)
                                .cursor_pointer()
                                .on_click(cx.listener(|t, _, _, cx| {
                                    t.error = None;
                                    cx.notify();
                                }))
                                .child(div().flex_1().min_w_0().child(error))
                                .child(div().flex_none().child("Dismiss")),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(main)),
            )
    }
}

fn centered(text: &str, color: Hsla) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color)
        .child(text.to_string())
        .into_any_element()
}
