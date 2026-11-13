//! What's in the dialog, which the hub keeps as one record of its own, files and all, so it's there after the app
//! restarts, and in every client: the dialog has the hub keep each change, shows a change made elsewhere, such as in
//! another client, and reads back the files it names. Quitting loses only what the hub hasn't had yet.

use std::collections::HashSet;

use client_core::{CameBack, Client, DraftChange, DraftKey, Drafts, FilesFor, KeptDraft, ReadBack};
use gpui_kit::{App, Context, Window};
use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, NewWorkspaceDraft, NewWorkspaceDraftToKeep, PermissionMode};
use protocol::prompt::says_or_carries;

use super::{DialogEvent, NewWorkspaceDialog};
use crate::attached::read_back;

/// What keeps the dialog on the hub, and the connection its files are read back on.
#[derive(Default)]
pub(crate) struct Keeping {
    keeper: Option<Drafts>,
    client: Option<Client>,
    /// The hub's record as it was last told of, to compare what's here with.
    kept: NewWorkspaceDraft,
    /// Files the record names that haven't been read back yet, in its order, which come before any here.
    restoring: Vec<Attachment>,
}

impl Keeping {
    pub(crate) fn new(keeper: Option<Drafts>, client: Option<Client>) -> Self {
        Self { keeper, client, ..Self::default() }
    }
}

impl NewWorkspaceDialog {
    /// Has the hub keep what's in the dialog, once typing pauses, or at once for a change nobody's still typing, such
    /// as something picked, or the dialog emptied by Create. Each file the hub hasn't been given goes first.
    pub(super) fn keep(&mut self, at_once: bool, cx: &App) {
        let Some(keeper) = self.keeping.keeper.clone() else { return };
        for file in self.attached.not_yet_given(&FilesFor::NewWorkspace) {
            keeper.attach(FilesFor::NewWorkspace, file);
        }
        let draft = DraftChange::NewWorkspace(self.to_keep(cx));
        if at_once {
            keeper.set(draft);
        } else {
            keeper.typed(draft);
        }
    }

    /// Has the hub let go of files it was given that the dialog no longer has, such as one taken out, or those a
    /// workspace being made takes with it. It does so after the draft that no longer names them, since the hub keeps a
    /// file that's named.
    pub(super) fn let_go_of(&self, files: &[AttachmentId]) {
        let Some(keeper) = &self.keeping.keeper else { return };
        for file in files {
            keeper.detach(FilesFor::NewWorkspace, file);
        }
    }

    /// What's here, as the hub keeps it. Files still being read back come first, where they'll be put back, so the
    /// draft goes on naming them.
    pub(super) fn to_keep(&self, cx: &App) -> NewWorkspaceDraftToKeep {
        let coming_back = self.keeping.restoring.iter().map(|file| file.id.clone());
        NewWorkspaceDraftToKeep {
            repo_id: Some(self.repo.clone()),
            text: self.prompt.read(cx).text().to_string(),
            files: coming_back.chain(self.attached.ids()).collect(),
            model: self.model.clone(),
            effort: self.effort,
            plan: self.planning(),
        }
    }

    /// Whether what's here is what the record says, its files in whatever order. What it doesn't say, such as a
    /// repository it names that isn't offered, can't differ.
    fn has_kept(&self, kept: &NewWorkspaceDraft, cx: &App) -> bool {
        let here = self.to_keep(cx);
        let (files, there): (HashSet<_>, HashSet<_>) =
            (here.files.into_iter().collect(), kept.files.iter().map(|file| file.id.clone()).collect());
        let repo = kept.repo_id.as_ref().filter(|repo| self.offers(repo));
        here.text == kept.text
            && files == there
            && repo.is_none_or(|repo| *repo == self.repo)
            && kept.model.as_ref().is_none_or(|model| Some(model) == self.model.as_ref())
            && kept.effort.is_none_or(|effort| Some(effort) == self.effort)
            && kept.plan == self.planning()
    }

    fn offers(&self, repo: &protocol::model::RepoId) -> bool {
        self.offer.repos.iter().any(|(id, _)| id == repo)
    }

    /// Shows what the hub keeps of the dialog in place of what's here, when it was changed elsewhere since, or before
    /// the app last started, as [`Drafts::from_elsewhere`] says. What's on its way to the hub from here
    /// goes on and replaces it, so what's typed here is never undone as it's typed. Whatever comes in its place is the
    /// hub's already, so none of it is kept again, and this client says it shows it, as taking a chat's draft does.
    pub(crate) fn take_kept(&mut self, kept: NewWorkspaceDraft, window: &mut Window, cx: &mut Context<Self>) {
        let keeper = self.keeping.keeper.as_ref();
        let elsewhere = keeper.is_some_and(|keeper| keeper.from_elsewhere(&KeptDraft::new_workspace(&kept)));
        if let Some(keeper) = keeper.filter(|_| elsewhere) {
            keeper.took(KeptDraft::new_workspace(&kept));
        }
        let taken = elsewhere && !self.has_kept(&kept, cx);
        self.keeping.kept = kept;
        if !taken {
            return;
        }
        let kept = self.keeping.kept.clone();
        if let Some(repo) = kept.repo_id.filter(|repo| self.offers(repo)) {
            self.repo = repo;
        }
        if let Some(model) = kept.model {
            self.model = Some(model);
        }
        if let Some(effort) = kept.effort {
            self.effort = Some(effort);
        }
        self.mode = if kept.plan { PermissionMode::Plan } else { PermissionMode::FullAccess };
        // A file it stopped naming is let go of by whoever took it out.
        let named: HashSet<AttachmentId> = kept.files.iter().map(|file| file.id.clone()).collect();
        self.attached.retain(|file| named.contains(file));
        let here: HashSet<AttachmentId> = self.attached.ids().collect();
        self.keeping.restoring = kept.files.into_iter().filter(|file| !here.contains(&file.id)).collect();
        if *self.prompt.read(cx).text() != kept.text {
            let end = kept.text.len();
            self.prompt.update(cx, |prompt, cx| {
                prompt.set_value(kept.text, window, cx);
                prompt.set_selected_range(end..end, cx);
            });
        }
        self.bring_back(cx);
        cx.notify();
    }

    /// Carries on on a new connection, reading back any files the last one couldn't.
    pub(crate) fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.keeping.client = Some(client.clone());
        self.bring_back(cx);
    }

    /// Reads back the files the record names that aren't here, off the main thread.
    fn bring_back(&self, cx: &mut Context<Self>) {
        let (Some(client), files) = (self.keeping.client.clone(), self.keeping.restoring.clone()) else { return };
        if files.is_empty() {
            return;
        }
        read_back(client, FilesFor::NewWorkspace, files, Self::brought_back, cx);
    }

    /// Puts the files read back in the dialog, before any attached since, then has the hub keep what's here if that
    /// isn't what its record says, such as with something typed meanwhile. Keeping it otherwise could undo a change
    /// made elsewhere that hasn't arrived yet. Without a connection, they're read again on the next.
    fn brought_back(&mut self, read: ReadBack, cx: &mut Context<Self>) {
        let Some(CameBack { files, unread }) = read.came_back(&self.keeping.restoring) else { return };
        self.keeping.restoring.clear();
        let mut problems: Vec<String> = unread.iter().map(ToString::to_string).collect();
        problems.extend(self.attached.bring_back(files, &FilesFor::NewWorkspace));
        for problem in problems {
            cx.emit(DialogEvent::Error(problem));
        }
        if !self.has_kept(&self.keeping.kept, cx) {
            self.keep(true, cx);
        }
        cx.notify();
    }

    /// Answers once what's handed to the keeper so far is kept, or once that's as far as it can get, such as without a
    /// connection, which Create waits for, so the hub makes the workspace from what's asked for.
    pub(super) fn kept_once_flushed(&self) -> Option<flume::Receiver<()>> {
        self.keeping.keeper.as_ref().map(Drafts::flushed)
    }

    /// Stops reading back the files the record names that aren't here, as Create takes them with it, and returns them.
    /// A read already on its way is dropped once it's back, as no longer waited for.
    pub(super) fn stop_bringing_back(&mut self) -> Vec<Attachment> {
        std::mem::take(&mut self.keeping.restoring)
    }

    /// Reads back `files` too, such as those a workspace that couldn't be made gives back, before any still being read
    /// back, leaving out any that's here already.
    pub(super) fn bring_back_too(&mut self, files: Vec<Attachment>, cx: &mut Context<Self>) {
        let here: HashSet<AttachmentId> = self.attached.ids().chain(self.restoring_ids()).collect();
        let mut files: Vec<Attachment> = files.into_iter().filter(|file| !here.contains(&file.id)).collect();
        if files.is_empty() {
            return;
        }
        files.append(&mut self.keeping.restoring);
        self.keeping.restoring = files;
        self.bring_back(cx);
    }

    fn restoring_ids(&self) -> impl Iterator<Item = AttachmentId> + '_ {
        self.keeping.restoring.iter().map(|file| file.id.clone())
    }

    /// Has the hub stop keeping what a workspace made a step at a time took from the dialog, for a hub too old to make
    /// one from what it keeps, which would empty it itself: what's here now is kept, and the files `made_of` named are
    /// let go of.
    pub(crate) fn made_without_the_hub(&mut self, made_of: &NewWorkspaceDraftToKeep, cx: &App) {
        self.keep(true, cx);
        self.let_go_of(&made_of.files);
    }

    /// Whether anything is typed or attached that quitting or restarting would lose, because the hub hasn't had it.
    pub(crate) fn would_lose(&self, cx: &App) -> bool {
        let unkept = self.keeping.keeper.as_ref().is_none_or(|keeper| keeper.unkept(&DraftKey::NewWorkspace));
        unkept && says_or_carries(&self.prompt.read(cx).text().to_string(), self.attached.len())
    }
}

/// What a test reads.
#[cfg(feature = "testing")]
impl NewWorkspaceDialog {
    /// The hub's record as the dialog was last told of it, for a test to see that it has heard of a change.
    pub fn kept(&self) -> &NewWorkspaceDraft {
        &self.keeping.kept
    }

    /// Takes `kept` as the hub's record, as the app does, for a test to act before what that starts has run.
    pub fn take_kept_now(&mut self, kept: NewWorkspaceDraft, window: &mut Window, cx: &mut Context<Self>) {
        self.take_kept(kept, window, cx);
    }
}
