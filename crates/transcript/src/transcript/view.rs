use super::drawing::row_of;
use super::{Model, events, find, outline, record};
use crate::attachments::Picture;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::list::TranscriptList;
use crate::questions::QuestionPanel;
use crate::references::References;
use crate::selection::{Asked, Selection};
use crate::texts::{Of, Texts};
use client_core::hints::HintCollector;
use client_core::session_log::{RecordedApprovals, SessionLog};
use client_core::{Client, OpenSession, View};
use events::TranscriptEvent;
use find::Finding;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement as _, StyleRefinement, Styled as _,
    Task, div,
};
use look::MovingMarks;
use protocol::attachment::Attachment;
use protocol::event::SessionEvent;
use protocol::model::{Seq, Session, SessionId, SubagentId};
use record::{AtWork, tick};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::DictationHints;

pub struct Transcript {
    pub(super) client: Client,
    pub(super) session_id: SessionId,
    /// What its list is known by, which differs between a chat and the subagent tab showing its work.
    pub(super) list_id: String,
    /// Client-core's copy of the session, which this only reads.
    pub(super) session: OpenSession,
    /// The version of the session's log the transcript last caught up with, which is what it shows: the list was told
    /// of its rows, every row of a frame is drawn from it, and everything else reads it, until the next catch up.
    pub(super) log: Arc<SessionLog>,
    pub(super) model: Rc<RefCell<Model>>,
    /// The images the agent's text points to.
    pub(super) linked: LinkedImages<Seq>,
    pub(super) references: References<Seq>,
    /// The texts the transcript draws from the kit's blocks, parsed.
    pub(super) texts: Texts,
    /// What's selected of them.
    pub(super) selection: Selection,
    pub(super) list: TranscriptList,
    pub(super) _changes: Task<()>,
    /// Draws the time so far again while the agent works, and how long the cache has left while it doesn't.
    pub(super) ticking: Option<Task<()>>,
    pub(super) outline: outline::Outline,
    /// The oldest question or tool's use waiting on an answer, asked in the composer's place.
    pub(super) asking: Option<Entity<QuestionPanel>>,
    /// Where each request that's asked in the composer's place while it waits is in the log. The transcript draws a
    /// question once it's answered.
    pub(super) waiting: Vec<Seq>,
    /// Finding text in the chat, while its bar is open.
    pub(super) finding: Option<Finding>,
    /// Workspace-wide dictation hints supplied by the host pane, such as the branch and changed files.
    pub(super) dictation_hints: Option<DictationHints>,
    /// Where the marks that move at the end of the chat were last painted, which [`Transcript::drawn`] paints them
    /// over, so their frames don't draw the transcript again.
    pub(super) moving: MovingMarks,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    /// A chat: the agent's own events, with each subagent's call drawn from its record. From a hub that can't read
    /// them apart, every event, with each subagent's work gathered into its call.
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        Self::start(client, session, View::Own, AtWork::of(session), cx)
    }

    /// The work of `subagent`, alone, for the subagent tab. Whether it's at work is its record's to say, once
    /// [`Transcript::set_subagents`] passes it on.
    pub fn subagent(client: Client, session: &Session, subagent: SubagentId, cx: &mut Context<Self>) -> Self {
        let transcript = Self::start(client, session, View::Subagent(subagent.clone()), AtWork::default(), cx);
        transcript.model.borrow_mut().alone = Some(subagent);
        transcript
    }

    pub(super) fn start(
        client: Client,
        session: &Session,
        view: View,
        at_work: AtWork,
        cx: &mut Context<Self>,
    ) -> Self {
        let list_id = match &view {
            View::Subagent(call) => format!("{}-{call}", session.id),
            View::Whole | View::Own => session.id.to_string(),
        };
        let open = OpenSession::start_view(&client, session, view);
        let log = open.version();
        let (rows, loaded) = rows_and_loaded(&log);
        let this = cx.weak_entity();
        let texts = Texts::new(move |of, cx| {
            let _ = this.update(cx, |transcript, cx| transcript.redrawn(of, cx));
        });
        let selection = Selection::new(texts.clone(), cx.focus_handle());
        let list = TranscriptList::new(selection.clone(), cx);
        list.set_rows(rows, loaded);
        let linked = LinkedImages::of_transcript(client.clone(), session.id.clone(), cx.weak_entity());
        let references = References::of_transcript(client.clone(), session.workspace_id.clone(), cx.weak_entity());
        let ticking = tick(&at_work, cx);
        let approvals = RecordedApprovals::of(session);
        let model = Rc::new(RefCell::new(Model {
            approvals,
            claims: session.agent_capabilities.clone(),
            ..Model::with(at_work)
        }));
        let signals = open.changed().clone();
        let following = cx.spawn(async move |this, cx| {
            while signals.recv_async().await.is_ok() {
                if this.update(cx, Transcript::catch_up).is_err() {
                    return;
                }
            }
        });
        Self {
            client,
            session_id: session.id.clone(),
            list_id,
            session: open,
            log,
            model,
            linked,
            references,
            texts,
            selection,
            list,
            _changes: following,
            ticking,
            outline: outline::Outline::default(),
            asking: None,
            waiting: Vec::new(),
            finding: None,
            dictation_hints: None,
            moving: MovingMarks::default(),
        }
    }

    /// The transcript, as wherever it's shown draws it: cached, so it's drawn again only when it changes, rather than
    /// whenever anything around it does, with the marks that move at the end of the chat painted over it, which it
    /// leaves only room for.
    pub fn drawn(this: &Entity<Self>, cx: &App) -> impl IntoElement {
        let over = this.read(cx).moving.over();
        let cached = this.clone().cached(StyleRefinement::default().size_full());
        div().relative().size_full().child(cached).child(over)
    }

    pub fn log(&self) -> &SessionLog {
        &self.log
    }

    /// Gives question answers in this transcript the workspace's own hint words when dictating.
    pub fn set_dictation_hints(&mut self, hints: impl Fn(&mut HintCollector, &App) + 'static) {
        self.dictation_hints = Some(Box::new(hints));
    }

    pub(crate) fn add_dictation_hints(&self, collector: &mut HintCollector, cx: &App) {
        if let Some(hints) = &self.dictation_hints {
            hints(collector, cx);
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn list(&self) -> &TranscriptList {
        &self.list
    }

    /// Carries on on a new connection to the hub, keeping what's shown and where.
    pub fn reconnect(&mut self, client: &Client) {
        self.session.reconnect(client);
        self.client = client.clone();
        self.find_on_new_connection();
        // Asked for again the next time they're drawn, in case the connection was what failed. The session says it
        // changed once it has caught up on the new connection, or lost it, and the transcript is drawn again then.
        self.model.borrow_mut().pictures.retain(|_, picture| !matches!(picture, Picture::Unreadable));
        self.linked.reconnect(client);
        self.references.reconnect(client);
    }

    /// Reads which workspace a pull request the chat names is in from `records`, client-core's copy of the hub's.
    pub fn follow_records(&mut self, records: client_core::RecordsFeed) {
        self.references.follow_records(records);
    }

    /// The approvals waiting on an answer, newest last.
    pub fn open_approvals(&self) -> Vec<SessionEvent> {
        let m = self.model.borrow();
        self.log.open_approvals(m.approvals.as_ref()).into_iter().cloned().collect()
    }

    pub fn loaded_events(&self) -> usize {
        self.log.loaded_events()
    }

    /// Rows in the list, not counting the streaming tail.
    pub fn row_count(&self) -> usize {
        self.log.row_count()
    }

    /// Copies what's selected of the chat's texts, drawn or not, or lets the action go on when nothing is.
    pub fn copy_selection(&self, cx: &mut App) {
        self.selection.copy(cx);
    }

    /// Scrolls the chat up a screen, as Page Up does in it, for the same key from the composer.
    pub fn page_up(&self, cx: &mut Context<Self>) {
        self.selection.paging().ask(Asked::Screens(-1.));
        cx.notify();
    }

    /// Scrolls the chat down a screen, as Page Down does in it, for the same key from the composer.
    pub fn page_down(&self, cx: &mut Context<Self>) {
        self.selection.paging().ask(Asked::Screens(1.));
        cx.notify();
    }

    /// Shows what changed in the session since this last looked, however much that is. It takes the log as it is now,
    /// with what changed since, and tells the list of its rows, so every frame until the next is drawn from a log
    /// whose changes the list has been told of.
    pub(super) fn catch_up(&mut self, cx: &mut Context<Self>) {
        let (log, changes) = self.session.catch_up();
        self.log = log;
        let errors = self.session.take_errors();
        // What's asked opens and closes with an event, a row of its own rather than the tail, or is fetched alone.
        let events = self.log.row_count();
        let events_changed = changes.rows.as_ref().is_some_and(|changed| changed.start < events);
        let (rows, loaded) = rows_and_loaded(&self.log);
        self.list.set_rows(rows, loaded);
        self.follow_retry();
        self.texts.let_go(&self.log, self.selection.held().as_ref());
        if let Some(changed) = changes.rows {
            // A failure of the agent's own changes how rows around it are drawn.
            let redrawn = crate::rows::redrawn_with(&self.log, changed);
            self.list.out_of_date(redrawn.clone());
            self.find_in_changes(redrawn, cx);
        }
        if events_changed || changes.requests_fetched {
            self.follow_question(cx);
        }
        // What the search has read since, and a match whose row was loading, which may be here now.
        self.find_news(cx);
        self.go_on(cx);
        for error in errors {
            cx.emit(TranscriptEvent::Error(error.to_string()));
        }
        if changes.turns_ended > 0 {
            cx.emit(TranscriptEvent::TurnEnded);
        }
        cx.notify();
    }

    /// Opens or closes a fold drawn in the row of the event at `seq`, which changes that row's height. The row stays
    /// where it is in the window, with the fold where it was clicked, as far as the chat's end lets it.
    pub(crate) fn set_open(&mut self, seq: Seq, key: ItemKey, fold: Fold, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, fold, open);
        if let Some(row) = self.log.row_of(seq) {
            self.list.keep_still(row, open);
        }
        self.remeasure(seq, cx);
    }

    /// Opens or closes a fold drawn at the chat's end, such as the line for several background tasks, which changes
    /// the last row's height. The row stays where it is in the window, as far as the chat's end lets it.
    pub(crate) fn set_open_at_end(&mut self, key: ItemKey, fold: Fold, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, fold, open);
        self.list.keep_still(self.list.row_count().saturating_sub(1), open);
        self.redraw_end();
        cx.notify();
    }

    /// Measures the row the event at `seq` is drawn in again, since what it draws has changed height, and draws it.
    pub(crate) fn remeasure(&mut self, seq: Seq, cx: &mut Context<Self>) {
        // Not loaded, so not measured either.
        if let Some(row) = self.log.row_drawing(seq) {
            self.list.out_of_date(row..row + 1);
            // What it draws has changed too.
            self.find_in_changes(row..row + 1, cx);
        }
        cx.notify();
    }

    /// Measures the row `of` is drawn in again, since it draws what it drew another way, and draws it.
    pub(crate) fn redrawn(&mut self, of: Of, cx: &mut Context<Self>) {
        if let Some(row) = row_of(&self.log, self.list.row_count(), of) {
            self.remeasure_row(row, cx);
        }
    }

    /// Measures `row` again, since what it draws has changed height, and draws it.
    pub(crate) fn remeasure_row(&mut self, row: usize, cx: &mut Context<Self>) {
        self.list.out_of_date(row..row + 1);
        cx.notify();
    }

    /// Whether thinking shows open, here and as it streams, rather than as a line to open. A block someone opened
    /// or closed stays as they left it.
    pub fn set_thinking_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !self.model.borrow_mut().folds.set_thinking_open(open) {
            return;
        }
        // Any row can hold thinking, so every row can change height.
        self.list.all_out_of_date();
        cx.notify();
    }

    /// Fetches an image a row is drawing, and measures the row again once it's here, since its shape can change the
    /// row's height: one a tool sent back is as tall as its shape makes it, and a prompt's is as wide as its shape
    /// makes it, which can take the next one onto another line.
    pub(crate) fn load_picture(&mut self, seq: Option<Seq>, attachment: Attachment, cx: &mut Context<Self>) {
        let (client, session) = (self.client.clone(), self.session_id.clone());
        cx.spawn(async move |this, cx| {
            let id = attachment.id.clone();
            let picture = cx
                .background_spawn(async move {
                    client
                        .attachment_read(&session, &attachment.id)
                        .map_or(Picture::Unreadable, |bytes| Picture::read(attachment.media_type, bytes))
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                t.model.borrow_mut().pictures.insert(id, picture);
                match seq {
                    Some(seq) => t.remeasure(seq, cx),
                    None => cx.notify(),
                }
            });
        })
        .detach();
    }
}

/// How many rows the list has, and which of them are loaded. The row after the log is the streaming tail, which is
/// always there, and always loaded. An empty one has no height.
pub(super) fn rows_and_loaded(log: &SessionLog) -> (usize, Vec<std::ops::Range<usize>>) {
    let rows = log.row_count() + 1;
    let mut loaded = log.loaded_rows();
    loaded.push(rows - 1..rows);
    (rows, loaded)
}
