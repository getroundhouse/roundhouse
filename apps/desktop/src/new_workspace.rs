//! The new-workspace dialog: which repository, what to work on, the model and its effort, plan mode, and files to go
//! with the first prompt. It asks the app to make the workspace with an event, and the app tells it where one can be
//! made and whether one can be made now, so it never reaches into the app. What's left in it is there the next time it
//! opens, in this app or another, since the hub keeps it, as `keeping` says.

use std::path::PathBuf;
use std::rc::Rc;

use client_core::{Agents, FileToSend, FilesFor};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, Hsla, ParentElement as _, Pixels, Styled as _,
    Subscription, Window, px,
};
use protocol::attachment::Attachment;
use protocol::model::{
    AgentDescription, AgentKind, AttachmentId, Effort, ModelName, NewWorkspaceDraftToKeep, PermissionMode, RepoId,
};
use protocol::prompt::{Prompt, says_or_carries};

use crate::attached::{self, Attachments, joined};
use crate::composing;
use crate::model_picker::Picked;

mod keeping;
mod view;

pub(crate) use keeping::Keeping;

/// Wide enough for a few lines of a task's description, and narrower than the transcript's column.
const WIDTH: Pixels = px(720.);
/// The composer's corners.
const RADIUS: Pixels = px(10.);

/// What Create asks the app to make.
#[derive(Debug)]
pub struct Request {
    pub repo: RepoId,
    /// What was typed and attached, which is sent once the workspace is made, unless it says nothing and carries
    /// nothing, as [`Request::has_prompt`] says. Its files are the ones the dialog had, to give back, or to send to a
    /// hub too old to make the workspace from what it keeps.
    pub prompt: Prompt<FileToSend>,
    /// Files the hub keeps for the dialog that hadn't been read back yet, which come before the prompt's.
    pub coming_back: Vec<Attachment>,
    /// The kind of agent its first chat runs.
    pub agent: AgentKind,
    /// The model and effort the dialog showed. `None` from a hub that doesn't describe its agents, with nothing picked,
    /// for what the agent starts a chat with.
    pub model: Option<ModelName>,
    pub effort: Option<Effort>,
    /// Plan mode, if it was on in the dialog, and full access otherwise.
    pub permission_mode: PermissionMode,
    /// What Create had the hub keep as what's in the dialog, which the hub makes the workspace from. It's what the
    /// dialog keeps, not what it shows, so its model and effort are `None` unless one was picked or kept, as the hub's
    /// copy says, and the hub can tell once the dialog says nothing else.
    pub made_of: NewWorkspaceDraftToKeep,
    /// Answers once the hub keeps `made_of` as what's in the dialog, or once that's as far as it can get, so the hub
    /// can make the workspace from what it keeps. `None` for a request that's not from the dialog.
    pub kept: Option<flume::Receiver<()>>,
}

impl Request {
    /// A request that's not from the dialog, such as a test's, which asks the hub to make what it says, with the model
    /// and effort it names, or with `None`, what the agent starts a chat with. The hub refuses any file it carries,
    /// since only the dialog gives the hub its files.
    #[cfg(feature = "testing")]
    pub fn new(
        repo: RepoId,
        prompt: Prompt<FileToSend>,
        agent: AgentKind,
        (model, effort): (Option<ModelName>, Option<Effort>),
        permission_mode: PermissionMode,
    ) -> Self {
        let made_of = NewWorkspaceDraftToKeep {
            repo_id: Some(repo.clone()),
            text: prompt.text.clone(),
            files: prompt.files.iter().map(|file| file.attachment.id.clone()).collect(),
            model: model.clone(),
            effort,
            plan: permission_mode == PermissionMode::Plan,
        };
        let coming_back = Vec::new();
        Self { repo, prompt, coming_back, agent, model, effort, permission_mode, made_of, kept: None }
    }

    /// Whether there's a prompt to send once the workspace is made, counting the files still coming back.
    pub fn has_prompt(&self) -> bool {
        says_or_carries(&self.prompt.text, self.coming_back.len() + self.prompt.files.len())
    }
}

/// What the app tells the dialog: the repositories a workspace can be made in, whether one can be made now, and what the
/// hub says of its agents, which the first chat runs one of.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    /// Each repository's ID and name, in the sidebar's order.
    pub(crate) repos: Vec<(RepoId, String)>,
    pub(crate) can_create: bool,
    pub(crate) agents: Agents,
}

/// What the dialog asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogEvent {
    /// Create was pressed while the app could make a workspace. The app takes the request with
    /// [`NewWorkspaceDialog::take_request`].
    Create,
    /// Something to tell the person, such as a file that couldn't be attached.
    Error(String),
}

pub struct NewWorkspaceDialog {
    /// Where the workspace will be made.
    repo: RepoId,
    offer: Offer,
    prompt: Entity<TextareaState>,
    /// What was picked, or kept by the hub. `None` until then, which is what the first chat's agent starts a chat with.
    model: Option<ModelName>,
    effort: Option<Effort>,
    /// The mode the first chat starts in: full access for each new workspace, unless plan mode is turned on, since it
    /// belongs to the task at hand.
    mode: PermissionMode,
    attached: Attachments,
    keeping: Keeping,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DialogEvent> for NewWorkspaceDialog {}

/// Opens the dialog over the window, with its prompt focused. `closed` runs once it's closed, however it closes.
pub(crate) fn open(
    dialog: &Entity<NewWorkspaceDialog>,
    closed: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let (shown, closed) = (dialog.clone(), Rc::new(closed));
    window.open_dialog(cx, move |surface, _, cx| {
        // The composer's corners, since it's where a prompt is written. The dialog's own padding is left to what's in
        // it, which runs a line under the repository from edge to edge.
        let ((background, edge), closed) = (shown.read(cx).colors(cx), closed.clone());
        surface
            .p_0()
            .w(WIDTH)
            .close_button(false)
            .bg(background)
            .border_color(edge)
            .rounded(RADIUS)
            .child(shown.clone())
            // Enter is the prompt's, which creates, and Create closes the dialog once it can act, so Enter pressed
            // while it waits leaves the dialog open.
            .on_ok(|_, _, _| false)
            .on_close(move |_, window, cx| closed(window, cx))
    });
    // After opening, which focuses the dialog itself.
    dialog.update(cx, |dialog, cx| dialog.prompt.update(cx, |prompt, cx| prompt.focus(window, cx)));
}

impl NewWorkspaceDialog {
    pub(crate) fn new(
        repo: RepoId,
        offer: Offer,
        keeping: Keeping,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(7, 16)
                .submit_on_enter(true)
                .placeholder("What do you want to work on?")
        });
        let typed = cx.subscribe_in(&prompt, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.create(cx),
            InputEvent::Change => this.keep(false, cx),
            InputEvent::PressEnter { shift: true, .. } | InputEvent::Focus | InputEvent::Blur => {}
        });
        let (model, effort, attached) = (None, None, Attachments::default());
        let mode = PermissionMode::FullAccess;
        Self { repo, offer, prompt, model, effort, mode, attached, keeping, _subscriptions: vec![typed] }
    }

    pub(crate) fn set_offer(&mut self, offer: Offer, cx: &mut Context<Self>) {
        if offer != self.offer {
            self.offer = offer;
            cx.notify();
        }
    }

    pub(crate) fn set_repo(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        if repo != self.repo {
            self.repo = repo;
            self.keep(true, cx);
            cx.notify();
        }
    }

    /// Asks the app to make the workspace. Nothing happens while it can't, such as while it's making or archiving
    /// another, which Create shows by being disabled.
    fn create(&mut self, cx: &mut Context<Self>) {
        if self.can_create() {
            cx.emit(DialogEvent::Create);
        }
    }

    fn can_create(&self) -> bool {
        self.offer.can_create
    }

    /// Empties the dialog of what it was asked to make, and turns plan mode off, keeping the repository, model and
    /// effort for the next. The hub goes on keeping what's asked for, files and all, until the new workspace's first
    /// chat has it, so it's kept at once rather than emptied, and the request says once it has been. Emptying the
    /// prompt here isn't a change the hub hears of, since setting a field's text isn't typing.
    pub(crate) fn take_request(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Request {
        self.keep(true, cx);
        let made_of = self.to_keep(cx);
        let kept = self.kept_once_flushed();
        let text = self.prompt.read(cx).text().to_string();
        self.prompt.update(cx, |field, cx| field.set_value("", window, cx));
        let (repo, (model, effort)) = (self.repo.clone(), self.to_run());
        let permission_mode = std::mem::replace(&mut self.mode, PermissionMode::FullAccess);
        let files = self.attached.take();
        let coming_back = self.stop_bringing_back();
        cx.notify();
        let agent = self.offer.agents.for_new_chat();
        let prompt = Prompt::carrying(text, files);
        Request { repo, prompt, coming_back, agent, model, effort, permission_mode, made_of, kept }
    }

    /// Puts back a prompt for a workspace that couldn't be made, before anything typed or attached since, and plan
    /// mode if it was to be planned, so nothing is lost. Files that were still coming back from the hub are read back.
    pub(crate) fn give_back(
        &mut self,
        prompt: Prompt<FileToSend>,
        coming_back: Vec<Attachment>,
        mode: PermissionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Prompt { text, files } = prompt;
        if mode == PermissionMode::Plan {
            self.mode = mode;
        }
        for problem in self.attached.put_back(files) {
            cx.emit(DialogEvent::Error(problem));
        }
        self.bring_back_too(coming_back, cx);
        if !text.is_empty() {
            let text = joined(text, &self.prompt.read(cx).text().to_string());
            self.prompt.update(cx, |prompt, cx| prompt.set_value(text, window, cx));
        }
        self.keep(true, cx);
        cx.notify();
    }

    /// Takes a paste that holds images or copied files, and leaves text to the prompt. Returns whether it took it.
    fn paste(item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let Some(reading) = attached::pasted(item) else { return false };
        attached::read_into(reading, cx, |t| &mut t.attached, Self::attached);
        true
    }

    /// Files dropped on the dialog, or chosen from its `+` menu.
    fn attach_paths(paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        attached::read_paths_into(paths, cx, |t| &mut t.attached, Self::attached);
    }

    /// Once files have come, has the hub keep them, and says why each that couldn't be attached wasn't.
    fn attached(&mut self, problems: Vec<String>, cx: &mut Context<Self>) {
        self.keep(true, cx);
        for problem in problems {
            cx.emit(DialogEvent::Error(problem));
        }
    }

    /// Takes a file out, and has the hub let go of it once what's kept no longer names it.
    fn remove_attached(&mut self, id: &AttachmentId, cx: &mut Context<Self>) {
        let kept_in = self.attached.remove(id);
        self.keep(true, cx);
        if kept_in == Some(FilesFor::NewWorkspace) {
            self.let_go_of(std::slice::from_ref(id));
        }
        cx.notify();
    }

    fn planning(&self) -> bool {
        self.mode == PermissionMode::Plan
    }

    /// What the hub says of the agent the first chat runs. `None` from a hub that doesn't describe it.
    fn agent(&self) -> Option<&AgentDescription> {
        self.offer.agents.new_chat()
    }

    /// The model and effort the first chat is to run: what was picked or kept, or else what its agent starts a chat
    /// with. Either is `None` from a hub that doesn't say, until it's picked or kept, which leaves it to the hub.
    fn to_run(&self) -> (Option<ModelName>, Option<Effort>) {
        let defaults = self.agent().map(|agent| &agent.defaults);
        let model = self.model.clone().or_else(|| defaults.map(|defaults| defaults.model.clone()));
        (model, self.effort.or_else(|| defaults.map(|defaults| defaults.effort)))
    }

    /// The same, when both are known, for the picker to show.
    fn shown(&self) -> Option<(ModelName, Effort)> {
        let (model, effort) = self.to_run();
        Some((model?, effort?))
    }

    /// Its surface, and its edge, which the line under the repository is too: the composer's surface and a dialog's
    /// edge, or in plan mode the composer's copper, on which a dialog's own line would hardly show.
    fn colors(&self, cx: &App) -> (Hsla, Hsla) {
        composing::colors(self.planning(), cx.theme().border, cx)
    }

    /// Plan mode on or off for the first chat, as `Shift-Tab` and the `+` menu do.
    fn toggle_plan_mode(&mut self, cx: &mut Context<Self>) {
        self.mode = if self.planning() { PermissionMode::FullAccess } else { PermissionMode::Plan };
        self.keep(true, cx);
        cx.notify();
    }

    /// Takes what was picked in the model picker. Closing the menu gives focus back to the prompt, so typing carries
    /// on there.
    fn pick(&mut self, picked: Picked, cx: &mut Context<Self>) {
        match picked {
            Picked::Model(model) => self.model = Some(model),
            Picked::Effort(effort) => self.effort = Some(effort),
        }
        self.keep(true, cx);
        cx.notify();
    }
}

/// What a test reads, and the one choice a test makes that no menu offers.
#[cfg(feature = "testing")]
impl NewWorkspaceDialog {
    pub fn repo(&self) -> &RepoId {
        &self.repo
    }

    pub fn prompt_text(&self, cx: &App) -> String {
        self.prompt.read(cx).text().to_string()
    }

    /// The names of what's attached, in order, for a test to see what a paste or drop did.
    pub fn attached_names(&self) -> Vec<String> {
        self.attached.names()
    }

    /// The model and effort shown, as picked, kept, or what the agent starts a chat with.
    pub fn chosen(&self) -> Option<(ModelName, Effort)> {
        self.shown()
    }

    /// The mode the first chat is to start in.
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    /// Chooses a level, such as one a hub doesn't know, which no menu offers.
    pub fn choose_effort(&mut self, effort: Effort, cx: &mut Context<Self>) {
        self.effort = Some(effort);
        self.keep(true, cx);
        cx.notify();
    }
}
