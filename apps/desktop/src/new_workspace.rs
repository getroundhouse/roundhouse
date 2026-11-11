//! The new-workspace dialog: which repository, what to work on, the model and its effort, and files to go with the
//! first prompt. It asks the app to make the workspace with an event, and the app tells it where one can be made and
//! whether one can be made now, so it never reaches into the app. What's left in it when it closes is there the next
//! time it opens.

use std::path::PathBuf;
use std::rc::Rc;

use client_core::FileToSend;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, ParentElement as _, Pixels, Styled as _,
    Subscription, Window, px,
};
use protocol::model::{AttachmentId, DEFAULT_EFFORT, DEFAULT_MODEL, Effort, ModelName, RepoId};

use crate::attached::{self, Attachments, joined};
use crate::model_picker::Picked;

mod view;

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
}

impl Request {
    /// Whether there's a prompt to send once the workspace is made.
    pub fn has_prompt(&self) -> bool {
        attached::anything_to_send(&self.prompt, self.files.len())
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
    attached: Attachments,
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
        // The composer's surface and corners, since it's where a prompt is written. The dialog's own padding is
        // left to what's in it, which runs a line under the repository from edge to edge.
        let (background, closed) = (cx.theme().muted, closed.clone());
        surface
            .p_0()
            .w(WIDTH)
            .close_button(false)
            .bg(background)
            .rounded(RADIUS)
            .child(shown.clone())
            .on_close(move |_, window, cx| closed(window, cx))
    });
    // After opening, which focuses the dialog itself.
    dialog.update(cx, |dialog, cx| dialog.prompt.update(cx, |prompt, cx| prompt.focus(window, cx)));
}

impl NewWorkspaceDialog {
    pub(crate) fn new(repo: RepoId, offer: Offer, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(7, 16)
                .submit_on_enter(true)
                .placeholder("What do you want to work on?")
        });
        let on_enter = cx.subscribe_in(&prompt, window, |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.create(cx);
            }
        });
        let (model, effort, attached) = (DEFAULT_MODEL, DEFAULT_EFFORT, Attachments::default());
        Self { repo, offer, prompt, model, effort, attached, _subscriptions: vec![on_enter] }
    }

    pub(crate) fn set_offer(&mut self, offer: Offer, cx: &mut Context<Self>) {
        if offer != self.offer {
            self.offer = offer;
            cx.notify();
        }
    }

    pub(crate) fn set_repo(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        self.repo = repo;
        cx.notify();
    }

    /// Asks the app to make the workspace. Nothing happens while it can't, such as while it's making or archiving
    /// another, which Create shows by being disabled.
    fn create(&mut self, cx: &mut Context<Self>) {
        if self.offer.can_create {
            cx.emit(DialogEvent::Create);
        }
    }

    /// Empties the dialog of what it was asked to make, keeping the repository, model and effort for the next.
    pub(crate) fn take_request(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Request {
        let prompt = self.prompt.read(cx).text().to_string();
        self.prompt.update(cx, |field, cx| field.set_value("", window, cx));
        let (repo, model, effort) = (self.repo.clone(), self.model.clone(), self.effort);
        cx.notify();
        Request { repo, prompt, files: self.attached.take(), model, effort }
    }

    /// Puts back a prompt for a workspace that couldn't be made, before anything typed or attached since, so nothing
    /// is lost.
    pub(crate) fn give_back(
        &mut self,
        text: String,
        files: Vec<FileToSend>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for problem in self.attached.put_back(files) {
            cx.emit(DialogEvent::Error(problem));
        }
        if !text.is_empty() {
            let text = joined(text, &self.prompt.read(cx).text().to_string());
            self.prompt.update(cx, |prompt, cx| prompt.set_value(text, window, cx));
        }
        cx.notify();
    }

    /// Whether anything is typed or attached, which quitting or restarting would lose.
    pub(crate) fn has_anything(&self, cx: &App) -> bool {
        attached::anything_to_send(&self.prompt.read(cx).text().to_string(), self.attached.len())
    }

    /// Takes a paste that holds images or copied files, and leaves text to the prompt. Returns whether it took it.
    fn paste(item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let Some(reading) = attached::pasted(item) else { return false };
        attached::read_into(reading, cx, |t| &mut t.attached, |_, problem, cx| cx.emit(DialogEvent::Error(problem)));
        true
    }

    /// Files dropped on the dialog, or chosen with its `+`.
    fn attach_paths(paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        attached::read_paths_into(
            paths,
            cx,
            |t| &mut t.attached,
            |_, problem, cx| {
                cx.emit(DialogEvent::Error(problem));
            },
        );
    }

    fn remove_attached(&mut self, id: &AttachmentId, cx: &mut Context<Self>) {
        self.attached.remove(id);
        cx.notify();
    }

    /// Takes what was picked in the model picker. Closing the menu gives focus back to the prompt, so typing carries
    /// on there.
    fn pick(&mut self, picked: Picked, cx: &mut Context<Self>) {
        match picked {
            Picked::Model(model) => self.model = model,
            Picked::Effort(effort) => self.effort = effort,
        }
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

    /// Chooses a level, such as one a hub doesn't know, which no menu offers.
    pub fn choose_effort(&mut self, effort: Effort, cx: &mut Context<Self>) {
        self.effort = effort;
        cx.notify();
    }
}
