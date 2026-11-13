//! The transcript view: what it's showing, and how it answers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use client_core::session_log::{RecordedApprovals, SessionLog};
use client_core::{Client, OpenSession, View};
use gpui_kit::{
    AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, ParentElement as _, Render, SharedString, Task,
    Window,
};
use protocol::attachment::Attachment;
use protocol::event::SessionEvent;
use protocol::model::{AttachmentId, Seq, Session, SessionId, Subagent, ToolUseId};

use crate::attachments::Picture;
use crate::find::{FindLook, TextId};
use crate::folds::{Fold, Folds};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::list::TranscriptList;
use crate::long_texts::LongTexts;
use crate::prose::{Of, Prose};
use crate::questions::QuestionPanel;
use crate::rows::SubagentCalls;
use crate::selection::Selection;
pub use events::TranscriptEvent;
use find::Finding;

mod answers;
mod events;
mod find;
mod outline;
mod pull_request;
mod record;
mod sending;
#[cfg(feature = "testing")]
mod testing;

pub use pull_request::PullRequestStanding;
use record::tick;
pub(crate) use record::{AtWork, IdleCache};

pub(crate) struct Model {
    /// What the agent the chat runs is called, as the hub describes it. `None` until it has, and from a hub that
    /// doesn't.
    pub agent: Option<SharedString>,
    pub folds: Folds,
    /// The images prompts carry, by attachment, from the first time a row drew each.
    pub pictures: HashMap<AttachmentId, Picture>,
    /// Where the calls that started subagents are, as rows have looked for them.
    pub subagent_calls: RefCell<SubagentCalls>,
    /// What the agent is at, as the session's record says, which the transcript's last row draws.
    pub at_work: AtWork,
    /// The failed request whose card offers Retry, as [`crate::rows::retry_at`] says, as of the last look.
    pub retry_at: Option<Seq>,
    /// Where the workspace's pull request stands on GitHub, which a line for a wait on it says. `None` until it's
    /// known, and while there's none or nothing to say of it.
    pub pull_request: Option<PullRequestStanding>,
    /// The approvals the session's record says its agent is waiting on, which the cards and the question asked follow.
    /// None from a hub that doesn't say, whose are found among what's loaded.
    pub approvals: Option<RecordedApprovals>,
    /// Each subagent's record, by the call that started it, as the hub keeps them. None from a hub that doesn't. Each is
    /// shared with client-core's snapshot, since every frame's rows take the records of the calls they draw, and one
    /// can hold a call's whole input, such as all of a file it wrote.
    pub subagents: HashMap<ToolUseId, Arc<Subagent>>,
    /// The `seq` of the event each subagent's call was drawn in, so the row is measured again when its record
    /// changes. A call's row is measured only once it's been drawn, so one that isn't here needs nothing.
    pub subagent_rows: RefCell<HashMap<ToolUseId, Seq>>,
    /// In the subagent tab, the call that started the subagent it shows, alone.
    pub alone: Option<ToolUseId>,
    /// What's being found in the chat, for rows to draw.
    pub find: Rc<FindLook>,
    /// The Markdown the kit's text view draws with matches in each row the log holds, as counting found it, which a
    /// row keeps views of once it's drawn, so they draw them.
    pub found_markdown: HashMap<usize, FoundMarkdown>,
}

impl Model {
    /// One that's drawn nothing yet, for a chat or for its subagent tab.
    fn with(at_work: AtWork) -> Self {
        Model {
            agent: None,
            folds: Folds::default(),
            pictures: HashMap::new(),
            subagent_calls: RefCell::default(),
            at_work,
            retry_at: None,
            pull_request: None,
            approvals: None,
            subagents: HashMap::new(),
            subagent_rows: RefCell::default(),
            alone: None,
            find: Rc::default(),
            found_markdown: HashMap::new(),
        }
    }
}

/// An idle chat, for tests that draw rows without one.
#[cfg(test)]
impl Default for Model {
    fn default() -> Self {
        Model::with(AtWork::default())
    }
}

/// The Markdown texts in a row with matches.
pub(crate) type FoundMarkdown = Rc<[TextId]>;

pub struct Transcript {
    client: Client,
    session_id: SessionId,
    /// What its list is known by, which differs between a chat and the subagent tab showing its work.
    list_id: String,
    /// Client-core's copy of the session, which this only reads.
    session: OpenSession,
    model: Rc<RefCell<Model>>,
    /// The images the agent's text points to.
    linked: LinkedImages,
    /// Where the rows' text views are made, which keeps each long one.
    texts: LongTexts,
    /// The texts the transcript draws from the kit's blocks, parsed.
    prose: Prose,
    /// What's selected of them.
    selection: Selection,
    list: TranscriptList,
    _changes: Task<()>,
    /// Draws the time so far again while the agent works, and how long the cache has left while it doesn't.
    ticking: Option<Task<()>>,
    outline: outline::Outline,
    /// The oldest question waiting on an answer, to be asked in the composer's place.
    asking: Option<Entity<QuestionPanel>>,
    /// Where every question waiting on an answer is in the log. The transcript draws them once they're answered.
    waiting: Vec<Seq>,
    /// Finding text in the chat, while its bar is open.
    finding: Option<Finding>,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    /// A chat: the agent's own events, with each subagent's call drawn from its record. From a hub that can't read
    /// them apart, every event, with each subagent's work gathered into its call.
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        Self::start(client, session, View::Own, AtWork::of(session), cx)
    }

    /// The work of the subagent `call` started, alone, for the subagent tab. Whether it's at work is its record's to
    /// say, once [`Transcript::set_subagents`] passes it on.
    pub fn subagent(client: Client, session: &Session, call: ToolUseId, cx: &mut Context<Self>) -> Self {
        let transcript = Self::start(client, session, View::Subagent(call.clone()), AtWork::default(), cx);
        transcript.model.borrow_mut().alone = Some(call);
        transcript
    }

    fn start(client: Client, session: &Session, view: View, at_work: AtWork, cx: &mut Context<Self>) -> Self {
        let list_id = match &view {
            View::Subagent(call) => format!("{}-{call}", session.id),
            View::Whole | View::Own => session.id.to_string(),
        };
        let open = OpenSession::start_view(&client, session, view);
        let (rows, loaded) = open.read(rows_and_loaded);
        let this = cx.weak_entity();
        let prose = Prose::new(move |of, cx| {
            let _ = this.update(cx, |transcript, cx| transcript.redrawn(of, cx));
        });
        let selection = Selection::new(prose.clone(), cx.focus_handle());
        let list = TranscriptList::new(selection.clone(), cx);
        list.set_rows(rows, loaded);
        let linked = LinkedImages::new(client.clone(), session.id.clone(), cx.weak_entity());
        let ticking = tick(&at_work, cx);
        let approvals = RecordedApprovals::of(session);
        let model = Rc::new(RefCell::new(Model { approvals, ..Model::with(at_work) }));
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
            model,
            linked,
            texts: LongTexts::default(),
            prose,
            selection,
            list,
            _changes: following,
            ticking,
            outline: outline::Outline::default(),
            asking: None,
            waiting: Vec::new(),
            finding: None,
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
        // Asked for again the next time they're drawn, in case the connection was what failed.
        self.model.borrow_mut().pictures.retain(|_, picture| !matches!(picture, Picture::Unreadable));
        self.linked.reconnect(client);
    }

    /// The approvals waiting on an answer, newest last.
    pub fn open_approvals(&self) -> Vec<SessionEvent> {
        let m = self.model.borrow();
        self.session.read(|log| log.open_approvals(m.approvals.as_ref()).into_iter().cloned().collect())
    }

    pub fn loaded_events(&self) -> usize {
        self.session.read(SessionLog::loaded_events)
    }

    /// Rows in the list, not counting the streaming tail.
    pub fn row_count(&self) -> usize {
        self.session.read(SessionLog::row_count)
    }

    /// Shows what changed in the session since this last looked, however much that is.
    fn catch_up(&mut self, cx: &mut Context<Self>) {
        let changes = self.session.take_changes();
        let errors = self.session.take_errors();
        let (rows, loaded) = self.session.read(rows_and_loaded);
        // A question opens and closes with an event, a row of its own rather than the tail, or is fetched alone.
        let events = self.session.read(SessionLog::row_count);
        let events_changed = changes.rows.as_ref().is_some_and(|changed| changed.start < events);
        self.list.set_rows(rows, loaded);
        self.follow_retry();
        self.session.read(|log| self.texts.let_go(log));
        self.session.read(|log| self.prose.let_go(log, self.selection.held().as_ref()));
        if let Some(changed) = changes.rows {
            self.list.out_of_date(changed.clone());
            self.find_in_changes(changed, cx);
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
    /// where it is in the window, with the fold where it was clicked.
    pub(crate) fn set_open(&mut self, seq: Seq, key: ItemKey, fold: Fold, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, fold, open);
        if let Some(row) = self.session.read(|log| log.row_of(seq)) {
            self.list.keep_still(row);
        }
        self.remeasure(seq, cx);
    }

    /// Opens or closes a fold drawn at the chat's end, such as the line for several background tasks, which changes
    /// the last row's height. The row stays where it is in the window, unless the chat is keeping to its latest row.
    pub(crate) fn set_open_at_end(&mut self, key: ItemKey, fold: Fold, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, fold, open);
        self.list.keep_still(self.list.row_count().saturating_sub(1));
        self.redraw_end();
        cx.notify();
    }

    /// Measures the row the event at `seq` is drawn in again, since what it draws has changed height, and draws it.
    pub(crate) fn remeasure(&mut self, seq: Seq, cx: &mut Context<Self>) {
        // Not loaded, so not measured either.
        if let Some(row) = self.session.read(|log| log.row_drawing(seq)) {
            self.list.out_of_date(row..row + 1);
            // What it draws has changed too.
            self.find_in_changes(row..row + 1, cx);
        }
        cx.notify();
    }

    /// Measures the row `of` is drawn in again, since it draws what it drew another way, and draws it.
    pub(crate) fn redrawn(&mut self, of: Of, cx: &mut Context<Self>) {
        let row = match of {
            Of::Event(seq) => self.session.read(|log| log.row_of(seq)),
            Of::Head | Of::Tail | Of::Foot => {
                self.list.row_count().checked_sub(1).and_then(|tail| crate::rows::list_row_of(of, tail))
            }
        };
        if let Some(row) = row {
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

impl Render for Transcript {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.keep_revealing(cx);
        let (model, session, wanted) = (self.model.clone(), self.session.clone(), self.session.clone());
        let line_text = look::LineText::new(window, cx);
        let shared = crate::rows::Shared {
            this: cx.weak_entity(),
            linked: self.linked.clone(),
            line_text: line_text.clone(),
            texts: self.texts.clone(),
            prose: self.prose.clone(),
            selection: self.selection.clone(),
        };
        // The list's last row is the tail, even while the log has rows it hasn't been told of, such as a log by place
        // that has just learned how far it goes, whose first row would otherwise be drawn there and fetched.
        let tail = self.list.row_count().saturating_sub(1);
        let room_under = crate::rows::room_under(&self.model.borrow());
        let rows = self.list.render(
            ElementId::Name(self.list_id.clone().into()),
            move |ix, _, cx| {
                let item = crate::rows::Item { ix, tail };
                shared
                    .texts
                    .draw_row(ix, &shared.this, cx, |cx| crate::rows::render_row(&model, &session, &shared, item, cx))
            },
            move |row| wanted.want(row),
            room_under,
            window,
            cx,
        );
        let bar = self.find_bar(cx);
        let chat = self.with_outline(rows.into_any_element(), line_text, window, cx).children(bar);
        self.selection.with_keys(chat)
    }
}

/// How many rows the list has, and which of them are loaded. The row after the log is the streaming tail, which is
/// always there, and always loaded. An empty one has no height.
fn rows_and_loaded(log: &SessionLog) -> (usize, Vec<std::ops::Range<usize>>) {
    let rows = log.row_count() + 1;
    let mut loaded = log.loaded_rows();
    loaded.push(rows - 1..rows);
    (rows, loaded)
}
