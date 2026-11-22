//! What a chat knows of the issues and pull requests its texts name, as the hub sends them: those drawn in sight in its
//! last frame are what it shows, while it's shown, which the hub reads and sends again as they change, and those it drew
//! are drawn again.

use std::sync::{Arc, Weak};

use client_core::{GithubView, RecordsFeed};
use gpui_kit::{App, Task, WeakEntity};
use parking_lot::Mutex;
use protocol::client::github::GithubShown;
use protocol::model::{Reference, ReferenceAsk, ReferenceRead, Seq, WorkspaceId};

use super::book::Book;
use crate::linked_images::Waiter;
use crate::transcript::{Transcript, TranscriptEvent};

/// The references one chat's texts name, by what each asks, and the workspaces of the hub it's on, to say which a pull
/// request is in. `W` is what draws one, such as a transcript's row, which is told when what's known of it changes.
/// Cheap to clone.
pub(crate) struct References<W>(Arc<Shared<W>>);

impl<W> Clone for References<W> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

struct Shared<W> {
    /// The chat's workspace, whose repository a chip names a reference in by its number alone.
    workspace: WorkspaceId,
    known: Mutex<Book<W>>,
    /// Client-core's copy of the hub's records, whose workspaces say which a pull request is in, and whose part of
    /// what the hub says of GitHub has what's drawn, once it's given one.
    records: Mutex<Option<RecordsFeed>>,
    /// What this shows of GitHub, once it's drawn with the records followed, and what listens for it changing.
    github: Mutex<Option<Following>>,
    /// Tells what drew one that what's known of it changed.
    changed: Changed<W>,
    /// Whether what drew one is in sight, as a row out of sight is when it's drawn only to be measured.
    in_sight: InSight<W>,
    /// Shows a workspace, as its menu's Go to asks.
    go_to: GoTo,
}

type Changed<W> = Box<dyn Fn(Vec<W>, &mut App) + Send + Sync>;
type InSight<W> = Box<dyn Fn(&W, &App) -> bool + Send + Sync>;
type GoTo = Box<dyn Fn(WorkspaceId, &mut App) + Send + Sync>;

struct Following {
    view: GithubView,
    _listening: Task<()>,
}

/// Which workspace a pull request is in, as the hub's records say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tie {
    /// In none of the hub's workspaces, or an issue.
    None,
    /// The chat's own workspace's, on `branch`.
    ThisOne { branch: String },
    /// Another workspace's, by its ID, what it's called and its branch, and whether it's archived.
    Elsewhere { id: WorkspaceId, name: String, branch: String, archived: bool },
}

impl References<Seq> {
    /// The references the texts of the transcript `transcript`, of a chat in `workspace`, name, each of whose rows is
    /// measured again when what's known of one it draws changes, and which asks the app to show a workspace.
    pub(crate) fn of_transcript(workspace: WorkspaceId, transcript: WeakEntity<Transcript>) -> Self {
        let (changed, go_to, in_sight) = (transcript.clone(), transcript.clone(), transcript);
        Self::new(
            workspace,
            move |drawers, cx| {
                let _ = changed.update(cx, |transcript, cx| {
                    for seq in drawers {
                        transcript.remeasure(seq, cx);
                    }
                    cx.notify();
                });
            },
            move |workspace, cx| {
                let _ = go_to.update(cx, |_, cx| cx.emit(TranscriptEvent::GoToWorkspace { workspace }));
            },
            move |seq, cx| in_sight.upgrade().is_some_and(|transcript| transcript.read(cx).in_sight(*seq)),
        )
    }
}

impl<W: Waiter> References<W> {
    /// The references the texts of a chat in `workspace` name, where `changed` tells what drew one that what's known
    /// of it changed, which is measured again, `go_to` shows a workspace, and `in_sight` says whether what drew one is
    /// in sight.
    pub(crate) fn new(
        workspace: WorkspaceId,
        changed: impl Fn(Vec<W>, &mut App) + Send + Sync + 'static,
        go_to: impl Fn(WorkspaceId, &mut App) + Send + Sync + 'static,
        in_sight: impl Fn(&W, &App) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(Shared {
            workspace,
            known: Mutex::new(Book::new()),
            records: Mutex::new(None),
            github: Mutex::new(None),
            changed: Box::new(changed),
            go_to: Box::new(go_to),
            in_sight: Box::new(in_sight),
        }))
    }

    /// Reads which workspace a pull request is in, which repository the chat's is on, and what the hub says of what's
    /// drawn, from `records` from now on.
    pub(crate) fn follow_records(&self, records: RecordsFeed) {
        *self.0.records.lock() = Some(records);
    }

    /// What the hub said of `ask`, which `drawer` draws, or `None` until it has. What's drawn in a frame is what's
    /// shown once it's drawn, which the hub reads, and sends again as it changes.
    pub(crate) fn read(&self, ask: &ReferenceAsk, drawer: W, cx: &mut App) -> Option<ReferenceRead> {
        let said = self.0.github.lock().as_ref().and_then(|following| following.view.feed().reference(ask));
        let (read, first) = self.0.known.lock().drawn(ask, drawer, said);
        if first {
            self.settle_after(cx);
        }
        read
    }

    /// Says a frame is being drawn, so what's shown once it's drawn is what it draws, even when that's none.
    pub(crate) fn drawing(&self, cx: &mut App) {
        let first = !std::mem::replace(&mut self.0.known.lock().settling, true);
        if first {
            self.settle_after(cx);
        }
    }

    /// Says whether what draws them is shown, as a chat's transcript is while its tab is, so it shows none while it's
    /// not, and what it draws next once it is again. Returns whether it's shown again after it wasn't.
    pub(crate) fn set_shown(&self, shown: bool) -> bool {
        let (again, hides) = {
            let mut known = self.0.known.lock();
            let again = shown && known.hidden;
            known.hidden = !shown;
            let hides = !shown && !known.shown.is_empty();
            if hides {
                known.shown.clear();
            }
            (again, hides)
        };
        // Apart from the book's lock, as hearing from the hub takes them the other way round.
        if hides && let Some(following) = &*self.0.github.lock() {
            following.view.show(GithubShown::default());
        }
        again
    }

    /// Once this frame is drawn, takes what it drew in sight to be what's shown, and tells the hub if that changed.
    fn settle_after(&self, cx: &mut App) {
        let references = self.clone();
        cx.defer(move |cx| {
            let in_sight = |by: &W| (references.0.in_sight)(by, cx);
            let Some(shown) = references.0.known.lock().settled(in_sight) else { return };
            references.follow_github(cx);
            if let Some(following) = &*references.0.github.lock() {
                following.view.show(GithubShown { references: shown, ..GithubShown::default() });
            }
        });
    }

    /// Starts listening for what the hub says of what's shown changing, once the records are followed.
    fn follow_github(&self, cx: &mut App) {
        let mut github = self.0.github.lock();
        if github.is_some() {
            return;
        }
        let Some(records) = self.0.records.lock().clone() else { return };
        let view = records.github().view();
        let changed = view.changed().clone();
        let shared = Arc::downgrade(&self.0);
        let listening = cx.spawn(async move |cx| {
            while changed.recv_async().await.is_ok() {
                if !cx.update(|cx| heard(&shared, cx)) {
                    return;
                }
            }
        });
        *github = Some(Following { view, _listening: listening });
    }

    /// How a chip names what `ask` asks for: `#1021` in the repository the chat's workspace is on, as the hub's records
    /// say, and `owner/name#41` in another, as GitHub writes them.
    pub(crate) fn label(&self, ask: &ReferenceAsk) -> String {
        let own = self.0.records.lock().as_ref().and_then(|records| {
            let snapshot = records.snapshot();
            let workspace = snapshot.workspaces().iter().find(|workspace| workspace.id == self.0.workspace)?;
            let repo = snapshot.repos().iter().find(|repo| repo.id == workspace.repo_id)?;
            repo.origin.url()?.on_github()
        });
        let (repository, number) = (ask.repository.as_str(), ask.number);
        if own.is_some_and(|own| own.as_str().eq_ignore_ascii_case(repository)) {
            format!("#{number}")
        } else {
            format!("{repository}#{number}")
        }
    }

    /// Which workspace `reference` is in, if it's a pull request one of the hub's workspaces is on.
    pub(crate) fn tie(&self, reference: &Reference) -> Tie {
        let Some(records) = self.0.records.lock().clone() else { return Tie::None };
        let Some(about) = reference.about() else { return Tie::None };
        let snapshot = records.snapshot();
        // Its own, then one that isn't archived, then the newest archived, since a pull request can be in several.
        let Some(workspace) = (snapshot.workspaces().iter())
            .filter(|workspace| workspace.pull_request_linked().is_some_and(|link| link.url == about.url))
            .max_by_key(|workspace| {
                (workspace.id == self.0.workspace, workspace.archived_at_ms.is_none(), workspace.archived_at_ms)
            })
        else {
            return Tie::None;
        };
        let branch = workspace.branch.to_string();
        if workspace.id == self.0.workspace {
            return Tie::ThisOne { branch };
        }
        let name = workspace.title.as_ref().map_or_else(|| workspace.name.clone(), ToString::to_string);
        Tie::Elsewhere { id: workspace.id.clone(), name, branch, archived: workspace.archived_at_ms.is_some() }
    }

    /// Shows the workspace `id`.
    pub(crate) fn go_to(&self, id: WorkspaceId, cx: &mut App) {
        (self.0.go_to)(id, cx);
    }
}

/// Tells what drew each reference whose hub's word changed. Returns false once the references have gone.
fn heard<W: Waiter>(shared: &Weak<Shared<W>>, cx: &mut App) -> bool {
    let Some(shared) = shared.upgrade() else { return false };
    let drawers = {
        let github = shared.github.lock();
        let Some(following) = &*github else { return true };
        let feed = following.view.feed();
        shared.known.lock().heard(|ask| feed.reference(ask))
    };
    if !drawers.is_empty() {
        (shared.changed)(drawers, cx);
    }
    true
}
