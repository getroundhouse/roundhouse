//! What a chat knows of the issues and pull requests its texts name, as the hub found them, asked for as they're drawn,
//! a frame's at once, and again a minute after, for those still drawn.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use client_core::{Client, ClientError, RecordsFeed};
use gpui_kit::{App, AppContext as _, WeakEntity};
use parking_lot::Mutex;
use protocol::model::{Reference, ReferenceAsk, ReferenceRead, Seq, WorkspaceId};

use crate::linked_images::Waiter;
use crate::transcript::{Transcript, TranscriptEvent};

/// How long what was found of one is drawn before it's asked for again, while it's drawn, which the hub answers from
/// what it keeps until that's as old. *Unconfirmed.*
pub(crate) const ASK_AGAIN_AFTER: Duration = Duration::from_secs(60);

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
    /// Replaced when its owner reconnects.
    client: Mutex<Client>,
    known: Mutex<Book<W>>,
    /// Those to ask for once this frame is drawn.
    to_ask: Mutex<Vec<ReferenceAsk>>,
    /// Client-core's copy of the hub's records, whose workspaces say which a pull request is in, once it's given one.
    records: Mutex<Option<RecordsFeed>>,
    /// Tells what drew one that what's known of it changed, or that it's time to ask for it again.
    changed: Changed<W>,
    /// Shows a workspace, as its menu's Go to asks.
    go_to: GoTo,
    /// Whether the hub said it's too old to look them up, until it reconnects.
    unsupported: AtomicBool,
}

type Changed<W> = Box<dyn Fn(Vec<W>, &mut App) + Send + Sync>;
type GoTo = Box<dyn Fn(WorkspaceId, &mut App) + Send + Sync>;

/// What's known of each reference, by what it asks, which [`References`] asks the hub for as [`Book::drawn`] says.
struct Book<W>(HashMap<ReferenceAsk, Known<W>>);

struct Known<W> {
    /// What the hub said, or `None` until it has.
    read: Option<ReferenceRead>,
    /// When it was last asked for, or `None` if it never was.
    asked_at: Option<Instant>,
    asking: bool,
    /// What's drawn it since it was last asked for, to tell when what's known of it changes.
    drawn_by: Vec<W>,
}

impl<W: Waiter> Book<W> {
    /// What's known of `ask`, which `drawer` draws at `now`, and whether it's due to be asked for, which it's then
    /// taken to be: the first time, and once what was found is [`ASK_AGAIN_AFTER`] old, unless it's being asked for.
    fn drawn(&mut self, ask: &ReferenceAsk, drawer: W, now: Instant) -> (Option<ReferenceRead>, bool) {
        let known = self.0.entry(ask.clone()).or_insert_with(|| Known {
            read: None,
            asked_at: None,
            asking: false,
            drawn_by: Vec::new(),
        });
        if !known.drawn_by.contains(&drawer) {
            known.drawn_by.push(drawer);
        }
        let due = !known.asking
            && known.asked_at.is_none_or(|asked_at| now.saturating_duration_since(asked_at) >= ASK_AGAIN_AFTER);
        if due {
            known.asking = true;
            known.asked_at = Some(now);
        }
        (known.read.clone(), due)
    }

    /// Keeps what the hub said of each of `asks`, if it answered, and returns what's drawn them since they were last
    /// asked for, each once.
    fn took(&mut self, asks: &[ReferenceAsk], read: Option<Vec<ReferenceRead>>) -> Vec<W> {
        let answers = read.map_or_else(|| vec![None; asks.len()], |read| read.into_iter().map(Some).collect());
        for (ask, answer) in asks.iter().zip(answers) {
            let Some(known) = self.0.get_mut(ask) else { continue };
            known.asking = false;
            // The hub keeps what it found while GitHub can't be read, so this keeps it only while the hub can't say:
            // while it can't be reached, and once it's started again, and lost what it kept, until GitHub answers.
            match answer {
                Some(ReferenceRead::Unread { .. } | ReferenceRead::Unknown) | None if known.read.is_some() => {}
                Some(answer) => known.read = Some(answer),
                None => {}
            }
        }
        let mut drawers = Vec::new();
        for ask in asks {
            let Some(known) = self.0.get_mut(ask) else { continue };
            for drawer in std::mem::take(&mut known.drawn_by) {
                if !drawers.contains(&drawer) {
                    drawers.push(drawer);
                }
            }
        }
        drawers
    }

    /// Takes each to be due to be asked for when it's next drawn, as on a new connection.
    fn forget_asking(&mut self) {
        for known in self.0.values_mut() {
            known.asked_at = None;
        }
    }
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
    pub(crate) fn of_transcript(client: Client, workspace: WorkspaceId, transcript: WeakEntity<Transcript>) -> Self {
        let (changed, go_to) = (transcript.clone(), transcript);
        Self::new(
            client,
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
        )
    }
}

impl<W: Waiter> References<W> {
    /// The references the texts of a chat in `workspace` name, where `changed` tells what drew one that what's known
    /// of it changed, which is measured again, and has what's in view drawn again, which is all it does when it's told
    /// of nothing, and `go_to` shows a workspace.
    pub(crate) fn new(
        client: Client,
        workspace: WorkspaceId,
        changed: impl Fn(Vec<W>, &mut App) + Send + Sync + 'static,
        go_to: impl Fn(WorkspaceId, &mut App) + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(Shared {
            workspace,
            client: Mutex::new(client),
            known: Mutex::new(Book(HashMap::new())),
            to_ask: Mutex::new(Vec::new()),
            records: Mutex::new(None),
            changed: Box::new(changed),
            go_to: Box::new(go_to),
            unsupported: AtomicBool::new(false),
        }))
    }

    /// Carries on on a new connection, asking again for each as it's next drawn.
    pub(crate) fn reconnect(&self, client: &Client) {
        *self.0.client.lock() = client.clone();
        self.0.unsupported.store(false, Ordering::Relaxed);
        self.0.known.lock().forget_asking();
    }

    /// Reads which workspace a pull request is in, and which repository the chat's is on, from `records` from now on.
    pub(crate) fn follow_records(&self, records: RecordsFeed) {
        *self.0.records.lock() = Some(records);
    }

    /// What the hub said of `ask`, which `drawer` draws, or `None` until it has. Asks for it, once this frame is drawn,
    /// the first time, and again once what was found is [`ASK_AGAIN_AFTER`] old.
    pub(crate) fn read(&self, ask: &ReferenceAsk, drawer: W, cx: &mut App) -> Option<ReferenceRead> {
        // By the executor's clock, which a test moves on.
        let now = cx.background_executor().now();
        let (read, due) = self.0.known.lock().drawn(ask, drawer, now);
        if due && !self.0.unsupported.load(Ordering::Relaxed) {
            let first = {
                let mut to_ask = self.0.to_ask.lock();
                to_ask.push(ask.clone());
                to_ask.len() == 1
            };
            if first {
                let references = self.clone();
                cx.defer(move |cx| references.ask(cx));
            }
        }
        read
    }

    /// Asks the hub for what's waiting to be asked for, all at once, and tells what drew each that what's known of it
    /// changed, and a minute later has what's in view drawn again, so those still in view are asked for again.
    fn ask(&self, cx: &mut App) {
        let asks = std::mem::take(&mut *self.0.to_ask.lock());
        if asks.is_empty() {
            return;
        }
        let (client, workspace) = (self.0.client.lock().clone(), self.0.workspace.clone());
        let asked = asks.clone();
        let reading = cx.background_spawn(async move { client.references_read(&workspace, asked) });
        let references = self.clone();
        cx.spawn(async move |cx| {
            let read = reading.await;
            // A hub too old to look them up is never asked again, and each is left as it's written.
            let unsupported = read.as_ref().is_err_and(ClientError::is_unsupported);
            references.0.unsupported.store(unsupported, Ordering::Relaxed);
            let drawers = references.0.known.lock().took(&asks, read.ok());
            cx.update(|cx| (references.0.changed)(drawers, cx));
            if unsupported {
                return;
            }
            cx.background_executor().timer(ASK_AGAIN_AFTER).await;
            // Only drawn again, not measured, since measuring lays out a row out of view too, which would ask for
            // what it names, where only what's in view is to be asked for again.
            cx.update(|cx| (references.0.changed)(Vec::new(), cx));
        })
        .detach();
    }

    /// How a chip names `reference`: `#1021` in the repository the chat's workspace is on, as the hub's records say,
    /// and `owner/name#41` in another, as GitHub writes them.
    pub(crate) fn label(&self, reference: &Reference) -> String {
        let own = self.0.records.lock().as_ref().and_then(|records| {
            let snapshot = records.snapshot();
            let workspace = snapshot.workspaces().iter().find(|workspace| workspace.id == self.0.workspace)?;
            let repo = snapshot.repos().iter().find(|repo| repo.id == workspace.repo_id)?;
            repo.origin.url()?.on_github()
        });
        let (repository, number) = (reference.repository.as_str(), reference.number);
        if own.is_some_and(|own| own.as_str().eq_ignore_ascii_case(repository)) {
            format!("#{number}")
        } else {
            format!("{repository}#{number}")
        }
    }

    /// Which workspace `reference` is in, if it's a pull request one of the hub's workspaces is on.
    pub(crate) fn tie(&self, reference: &Reference) -> Tie {
        let Some(records) = self.0.records.lock().clone() else { return Tie::None };
        let snapshot = records.snapshot();
        // Its own, then one that isn't archived, then the newest archived, since a pull request can be in several.
        let Some(workspace) = (snapshot.workspaces().iter())
            .filter(|workspace| {
                workspace
                    .pull_request
                    .as_ref()
                    .and_then(|found| found.pull_request.as_ref())
                    .is_some_and(|pull_request| pull_request.url == reference.url)
            })
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

#[cfg(test)]
mod tests {
    use protocol::model::{GithubRepositoryName, IssueNumber, IssueState, ReferenceKind};

    use super::*;

    fn ask(number: u64) -> ReferenceAsk {
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(number).unwrap() }
    }

    fn found(title: &str) -> ReferenceRead {
        let repository = GithubRepositoryName::new("o", "r").unwrap();
        ReferenceRead::Found {
            reference: Box::new(Reference {
                repository,
                number: IssueNumber::try_from(12).unwrap(),
                kind: ReferenceKind::Issue { state: IssueState::Open },
                url: "https://github.com/o/r/issues/12".into(),
                title: title.into(),
                author: None,
                opened_at_ms: 0,
                comments: 0,
                labels: Vec::new(),
                body: String::new(),
            }),
        }
    }

    #[test]
    fn one_is_asked_for_once_until_what_was_found_is_old() {
        let (mut book, now) = (Book::<u32>(HashMap::new()), Instant::now());
        assert_eq!(book.drawn(&ask(12), 1, now), (None, true), "the first time it's drawn");
        assert!(!book.drawn(&ask(12), 2, now).1, "nor while it's being asked for");
        assert_eq!(book.took(&[ask(12)], Some(vec![found("A")])), [1, 2], "and each that drew it is told");
        assert_eq!(book.drawn(&ask(12), 3, now + ASK_AGAIN_AFTER / 2), (Some(found("A")), false));
        assert!(book.drawn(&ask(12), 1, now + ASK_AGAIN_AFTER).1, "until it's a minute old");
        book.took(&[ask(12)], Some(vec![found("B")]));
        book.forget_asking();
        assert_eq!(book.drawn(&ask(12), 1, now + ASK_AGAIN_AFTER), (Some(found("B")), true), "or on a new connection");
    }

    #[test]
    fn what_was_found_is_kept_while_the_hub_can_say_no_more() {
        let mut book = Book::<u32>(HashMap::new());
        let now = Instant::now();
        let mut answer = |read: Option<Vec<ReferenceRead>>| {
            book.drawn(&ask(12), 1, now);
            book.took(&[ask(12)], read);
            book.drawn(&ask(12), 1, now).0
        };
        assert_eq!(
            answer(Some(vec![ReferenceRead::Unread { message: "offline".into() }])),
            Some(ReferenceRead::Unread { message: "offline".into() })
        );
        assert_eq!(answer(Some(vec![found("A")])), Some(found("A")));
        assert_eq!(answer(None), Some(found("A")), "nor when it can't be reached");
        assert_eq!(answer(Some(vec![ReferenceRead::Unread { message: "offline".into() }])), Some(found("A")));
        assert_eq!(answer(Some(vec![ReferenceRead::Missing])), Some(ReferenceRead::Missing), "but gone, it's gone");
    }
}
