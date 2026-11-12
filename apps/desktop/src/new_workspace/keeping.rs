//! What's in the dialog, which the hub keeps as one record of its own, files and all, so it's there after the app
//! restarts, and in every client: the dialog has the hub keep each change, shows a change made elsewhere, such as in
//! another client, and reads back the files it names. Quitting loses only what the hub hasn't had yet.

use std::collections::HashSet;

use client_core::{Client, Drafts, Where};
use gpui_kit::{App, Context, Window};
use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, NewWorkspaceDraft, NewWorkspaceDraftToKeep, PermissionMode};
use protocol::prompt::says_or_carries;

use super::{DialogEvent, NewWorkspaceDialog};
use crate::attached::{ReadBack, came_back, read_back};

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
        for file in self.attached.not_yet_given(&Where::NewWorkspace) {
            keeper.attach_to_new_workspace(file);
        }
        let draft = self.to_keep(cx);
        if at_once {
            keeper.set_new_workspace(draft);
        } else {
            keeper.typed_new_workspace(draft);
        }
    }

    /// Has the hub let go of files it was given that the dialog no longer has, such as one taken out, or those a
    /// workspace being made takes with it. It does so after the draft that no longer names them, since the hub keeps a
    /// file that's named.
    pub(super) fn let_go_of(&self, files: &[AttachmentId]) {
        let Some(keeper) = &self.keeping.keeper else { return };
        for file in files {
            keeper.detach_from_new_workspace(file);
        }
    }

    /// What's here, as the hub keeps it. Files still being read back come first, where they'll be put back, so the
    /// draft goes on naming them.
    fn to_keep(&self, cx: &App) -> NewWorkspaceDraftToKeep {
        let coming_back = self.keeping.restoring.iter().map(|file| file.id.clone());
        NewWorkspaceDraftToKeep {
            repo_id: Some(self.repo.clone()),
            text: self.prompt.read(cx).text().to_string(),
            files: coming_back.chain(self.attached.ids()).collect(),
            model: Some(self.model.clone()),
            effort: Some(self.effort),
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
            && kept.model.as_ref().is_none_or(|model| *model == self.model)
            && kept.effort.is_none_or(|effort| effort == self.effort)
            && kept.plan == self.planning()
    }

    fn offers(&self, repo: &protocol::model::RepoId) -> bool {
        self.offer.repos.iter().any(|(id, _)| id == repo)
    }

    /// Shows what the hub keeps of the dialog in place of what's here, when it was changed elsewhere since, or before
    /// the app last started, as [`Drafts::new_workspace_from_elsewhere`] says. What's on its way to the hub from here
    /// goes on and replaces it, so what's typed here is never undone as it's typed. Whatever comes in its place is the
    /// hub's already, so none of it is kept again, and this client says it shows it, as taking a chat's draft does.
    pub(crate) fn take_kept(&mut self, kept: NewWorkspaceDraft, window: &mut Window, cx: &mut Context<Self>) {
        let keeper = self.keeping.keeper.as_ref();
        let elsewhere = keeper.is_some_and(|keeper| keeper.new_workspace_from_elsewhere(&kept));
        if let Some(keeper) = keeper.filter(|_| elsewhere) {
            keeper.took_new_workspace(&kept);
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
            self.model = model;
        }
        if let Some(effort) = kept.effort {
            self.effort = effort;
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

    /// Reads back the files the record names that aren't here, off the main thread. A read isn't a command quitting
    /// waits for, as it can be a lot of bytes, and the hub keeps them anyway.
    fn bring_back(&self, cx: &mut Context<Self>) {
        let (Some(client), files) = (self.keeping.client.clone(), self.keeping.restoring.clone()) else { return };
        if files.is_empty() {
            return;
        }
        let read = move |file: &AttachmentId| client.new_workspace_draft_file_read(file);
        read_back(files, read, Self::brought_back, cx);
    }

    /// Puts the files read back in the dialog, before any attached since, then has the hub keep what's here if that
    /// isn't what its record says, such as with something typed meanwhile. Keeping it otherwise could undo a change
    /// made elsewhere that hasn't arrived yet. Without a connection, they're read again on the next.
    fn brought_back(&mut self, read: Vec<ReadBack>, cx: &mut Context<Self>) {
        let Some((files, mut problems)) = came_back(&self.keeping.restoring, read) else { return };
        self.keeping.restoring.clear();
        problems.extend(self.attached.bring_back(files, &Where::NewWorkspace));
        for problem in problems {
            cx.emit(DialogEvent::Error(problem));
        }
        if !self.has_kept(&self.keeping.kept, cx) {
            self.keep(true, cx);
        }
        cx.notify();
    }

    /// Whether files the hub keeps are still being read back, which Create waits for, so they go with the prompt.
    pub(super) fn bringing_back(&self) -> bool {
        !self.keeping.restoring.is_empty()
    }

    /// Whether anything is typed or attached that quitting or restarting would lose, because the hub hasn't had it.
    pub(crate) fn would_lose(&self, cx: &App) -> bool {
        let unkept = self.keeping.keeper.as_ref().is_none_or(|keeper| keeper.unkept_in(&Where::NewWorkspace));
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
