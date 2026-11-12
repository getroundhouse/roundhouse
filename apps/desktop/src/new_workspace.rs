//! The new-workspace dialog: which repository, what to work on, the model and its effort, plan mode, and files to go
//! with the first prompt. It asks the app to make the workspace with an event, and the app tells it where one can be
//! made and whether one can be made now, so it never reaches into the app. What's left in it is there the next time it
//! opens, in this app or another, since the hub keeps it, as `keeping` says.

use std::path::PathBuf;
use std::rc::Rc;

use client_core::{FileToSend, Where};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, Hsla, ParentElement as _, Pixels, Styled as _,
    Subscription, Window, px,
};
use protocol::model::{AttachmentId, DEFAULT_EFFORT, DEFAULT_MODEL, Effort, ModelName, PermissionMode, RepoId};
use protocol::prompt::says_or_carries;

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
    /// What was typed. With nothing typed and no files, nothing is sent.
    pub prompt: String,
    pub files: Vec<FileToSend>,
    pub model: ModelName,
    pub effort: Effort,
    /// Plan mode, if it was on in the dialog, and full access otherwise.
    pub permission_mode: PermissionMode,
}

impl Request {
    /// Whether there's a prompt to send once the workspace is made.
    pub fn has_prompt(&self) -> bool {
        says_or_carries(&self.prompt, self.files.len())
    }
}

/// What the app tells the dialog: the repositories a workspace can be made in, and whether one can be made now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    /// Each repository's ID and name, in the sidebar's order.
    pub(crate) repos: Vec<(RepoId, String)>,
    pub(crate) can_create: bool,
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
    /// What the hub gives a chat that names neither, until another is picked.
    model: ModelName,
    effort: Effort,
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
        let (model, effort, attached) = (DEFAULT_MODEL, DEFAULT_EFFORT, Attachments::default());
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
    /// another, or while files the hub kept are still coming back, which Create shows by being disabled.
    fn create(&mut self, cx: &mut Context<Self>) {
        if self.can_create() {
            cx.emit(DialogEvent::Create);
        }
    }

    fn can_create(&self) -> bool {
        self.offer.can_create && !self.bringing_back()
    }

    /// Empties the dialog of what it was asked to make, and turns plan mode off, keeping the repository, model and
    /// effort for the next. The hub keeps it emptied at once, and lets go of the files it was given, which go with the
    /// first prompt.
    pub(crate) fn take_request(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Request {
        let prompt = self.prompt.read(cx).text().to_string();
        self.prompt.update(cx, |field, cx| field.set_value("", window, cx));
        let (repo, model, effort) = (self.repo.clone(), self.model.clone(), self.effort);
        let permission_mode = std::mem::replace(&mut self.mode, PermissionMode::FullAccess);
        let given: Vec<AttachmentId> = self.attached.given_for(&Where::NewWorkspace).collect();
        let files = self.attached.take();
        self.keep(true, cx);
        self.let_go_of(&given);
        cx.notify();
        Request { repo, prompt, files, model, effort, permission_mode }
    }

    /// Puts back a prompt for a workspace that couldn't be made, before anything typed or attached since, and plan
    /// mode if it was to be planned, so nothing is lost.
    pub(crate) fn give_back(
        &mut self,
        text: String,
        files: Vec<FileToSend>,
        mode: PermissionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mode == PermissionMode::Plan {
            self.mode = mode;
        }
        for problem in self.attached.put_back(files) {
            cx.emit(DialogEvent::Error(problem));
        }
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
        if kept_in == Some(Where::NewWorkspace) {
            self.let_go_of(std::slice::from_ref(id));
        }
        cx.notify();
    }

    fn planning(&self) -> bool {
        self.mode == PermissionMode::Plan
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
            Picked::Model(model) => self.model = model,
            Picked::Effort(effort) => self.effort = effort,
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

    /// The model and effort chosen.
    pub fn chosen(&self) -> (ModelName, Effort) {
        (self.model.clone(), self.effort)
    }

    /// The mode the first chat is to start in.
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    /// Chooses a level, such as one a hub doesn't know, which no menu offers.
    pub fn choose_effort(&mut self, effort: Effort, cx: &mut Context<Self>) {
        self.effort = effort;
        self.keep(true, cx);
        cx.notify();
    }
}
