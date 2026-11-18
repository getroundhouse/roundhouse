use super::{Request, keeping};
use crate::attached::{Attachments, joined};
use crate::{attached, composing};
use client_core::{Agents, FileToSend, FilesFor};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, Pixels,
    Subscription, Window, px,
};
use keeping::Keeping;
use protocol::attachment::Attachment;
use protocol::capability::settings::Effort;
use protocol::model::{AgentDescription, AgentKind, AttachmentId, ModelName, PermissionMode, RepoId};
use protocol::prompt::Prompt;
use std::path::PathBuf;
use std::rc::Rc;

/// Wide enough for a few lines of a task's description, and narrower than the transcript's column.
const WIDTH: Pixels = px(720.);

/// What the app tells the dialog: the repositories a workspace can be made in, whether one can be made now, what the
/// hub says of its agents, which the first chat runs one of, and what those that discover their models offer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    /// Each repository's ID and name, in the sidebar's order.
    pub(crate) repos: Vec<(RepoId, String)>,
    pub(crate) can_create: bool,
    pub(crate) agents: Agents,
    /// Every list of models the hub holds, in no order.
    pub(crate) model_lists: Vec<protocol::model::ModelList>,
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
    pub(super) model_lookup: crate::model_lookup::Lookup,
    /// Where the workspace will be made.
    pub(super) repo: RepoId,
    pub(super) offer: Offer,
    pub(super) prompt: Entity<TextareaState>,
    pub(super) dictation: Entity<dictation_ui::Field>,
    /// The draft outlives the dialog, so retaining it alone doesn't say whether its input can take focus.
    shown: bool,
    /// What was picked, or kept by the hub. `None` until then, which is what the first chat's agent starts a chat with.
    pub(super) chosen_agent: Option<AgentKind>,
    pub(super) model: Option<ModelName>,
    pub(super) effort: Option<Effort>,
    /// The mode the first chat starts in: full access for each new workspace, unless plan mode is turned on, since it
    /// belongs to the task at hand.
    pub(super) mode: PermissionMode,
    pub(super) attached: Attachments,
    pub(super) keeping: Keeping,
    pub(super) _subscriptions: Vec<Subscription>,
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
    look::open_plain_dialog(window, cx, move |_, _| {
        let (closing, closed) = (shown.clone(), closed.clone());
        look::plain_dialog(shown.clone(), WIDTH)
            // Enter is the prompt's, which creates, and Create closes the dialog once it can act, so Enter pressed
            // while it waits leaves the dialog open.
            .on_confirm(|_, _| false)
            .on_close(move |window, cx| {
                closing.update(cx, NewWorkspaceDialog::closed);
                closed(window, cx);
            })
    });
    // After opening, which focuses the dialog itself.
    dialog.update(cx, |dialog, cx| {
        dialog.shown = true;
        dialog.follow_models(false, cx);
        dialog.dictation.update(cx, |field, cx| field.set_enabled(true, cx));
        dialog.prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
    });
}

impl NewWorkspaceDialog {
    pub(crate) fn is_shown(&self) -> bool {
        self.shown
    }

    /// Both dismissal by the kit and successful creation end the visible input's lifetime.
    pub(crate) fn closed(&mut self, cx: &mut Context<Self>) {
        self.shown = false;
        self.dictation.update(cx, |field, cx| field.set_enabled(false, cx));
    }

    /// Gives the prompt synthetic audio in UI tests.
    #[cfg(feature = "testing")]
    pub fn hear(&self, audio: &[f32], cx: &App) {
        self.dictation.read(cx).hear(audio);
    }

    fn dictation_hints(&self, cx: &App) -> dictation_ui::Hints {
        let mut collector = client_core::hints::HintCollector::new();
        collector.add_prose(&self.prompt.read(cx).text().to_string());
        for name in self.attached.names() {
            collector.add_path(&name);
        }
        if let Some((_, selected)) = self.offer.repos.iter().find(|(id, _)| id == &self.repo) {
            collector.add_name(selected);
        }
        for (_, name) in &self.offer.repos {
            collector.add_name(name);
        }
        dictation_ui::Hints::new(collector.finish())
    }

    pub(crate) fn new(
        repo: RepoId,
        offer: Offer,
        keeping: Keeping,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 16)
                .submit_on_enter(true)
                .placeholder("What do you want to work on?")
        });
        let typed = cx.subscribe_in(&prompt, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.create(cx),
            InputEvent::Change => this.keep(false, cx),
            InputEvent::PressEnter { shift: true, .. } | InputEvent::Focus | InputEvent::Blur => {}
        });
        let dialog = cx.weak_entity();
        let dictation = cx.new(|cx| {
            dictation_ui::Field::new(prompt.clone(), look::Corner::Card, window, cx).with_hints(move |cx| {
                dialog.upgrade().map_or_else(dictation_ui::Hints::default, |d| d.read(cx).dictation_hints(cx))
            })
        });
        let dictated = cx.subscribe(&dictation, |this, _, event, cx| match event {
            dictation_ui::Event::Ended { submit: true } => this.create(cx),
            dictation_ui::Event::Ended { submit: false } => {}
            dictation_ui::Event::Error(message) => cx.emit(DialogEvent::Error(message.clone())),
        });
        let live = cx.observe(&dictation, |_, _, cx| cx.notify());
        let readiness = crate::dictating::observe(cx, |_, cx| cx.notify());
        let (model, effort, attached) = (None, None, Attachments::default());
        let mode = PermissionMode::FullAccess;
        Self {
            repo,
            offer,
            prompt,
            model_lookup: crate::model_lookup::Lookup::default(),
            dictation,
            shown: false,
            chosen_agent: None,
            model,
            effort,
            mode,
            attached,
            keeping,
            _subscriptions: vec![typed, dictated, live, readiness],
        }
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
            self.follow_models(false, cx);
            self.keep(true, cx);
            cx.notify();
        }
    }

    /// Asks the app to make the workspace. Nothing happens while it can't, such as while it's making or archiving
    /// another, which Create shows by being disabled.
    pub(super) fn create(&mut self, cx: &mut Context<Self>) {
        if self.can_create() {
            // Creating without a first prompt is valid too, so even an empty recording must finish this action.
            if self.dictation.update(cx, |field, cx| field.finish_before_submit(true, cx)) {
                return;
            }
            cx.emit(DialogEvent::Create);
        }
    }

    pub(super) fn can_create(&self) -> bool {
        self.offer.can_create
            && self.base_agent().is_none_or(|agent| !agent.discovers_models() || self.selection_available())
    }

    /// Empties the dialog of what it was asked to make, and turns plan mode off, keeping the repository, model and
    /// effort for the next. The hub goes on keeping what's asked for, files and all, until the new workspace's first
    /// chat has it, so it's kept at once rather than emptied, and the request says once it has been. Emptying the
    /// prompt here isn't a change the hub hears of, since setting a field's text isn't typing.
    pub(crate) fn take_request(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Request {
        if self.base_agent().is_some_and(AgentDescription::discovers_models) {
            (self.model, self.effort) = self.to_run();
        }
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
        let agent = self.chosen_agent.unwrap_or_else(|| self.offer.agents.for_new_chat());
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
    pub(super) fn paste(item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let Some(reading) = attached::pasted(item) else { return false };
        attached::read_into(reading, cx, |t| &mut t.attached, Self::attached);
        true
    }

    /// Files dropped on the dialog, or chosen from its `+` menu.
    pub(super) fn attach_paths(paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        attached::read_paths_into(paths, cx, |t| &mut t.attached, Self::attached);
    }

    /// Once files have come, has the hub keep them, and says why each that couldn't be attached wasn't.
    pub(super) fn attached(&mut self, problems: Vec<String>, cx: &mut Context<Self>) {
        self.keep(true, cx);
        for problem in problems {
            cx.emit(DialogEvent::Error(problem));
        }
    }

    /// Takes a file out, and has the hub let go of it once what's kept no longer names it.
    pub(super) fn remove_attached(&mut self, id: &AttachmentId, cx: &mut Context<Self>) {
        let kept_in = self.attached.remove(id);
        self.keep(true, cx);
        if kept_in == Some(FilesFor::NewWorkspace) {
            self.let_go_of(std::slice::from_ref(id));
        }
        cx.notify();
    }

    pub(super) fn planning(&self) -> bool {
        self.mode == PermissionMode::Plan
    }

    /// Its surface, and its edge, which the line under the repository is too: the composer's surface and a dialog's
    /// edge, or in plan mode the composer's copper, on which a dialog's own line would hardly show.
    pub(super) fn colors(&self, cx: &App) -> (Hsla, Hsla) {
        composing::colors(self.planning(), look::colors(cx).edge, cx)
    }

    /// Plan mode on or off for the first chat, as `Shift-Tab` and the `+` menu do.
    pub(super) fn toggle_plan_mode(&mut self, cx: &mut Context<Self>) {
        self.mode = if self.planning() { PermissionMode::FullAccess } else { PermissionMode::Plan };
        self.keep(true, cx);
        cx.notify();
    }
}

impl Focusable for NewWorkspaceDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.prompt.focus_handle(cx)
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
