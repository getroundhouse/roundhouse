//! The transcript view: what it's showing, and how it answers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use client_core::session_log::{PromptState, Row, SessionLog};
use client_core::{Client, FileToSend, OpenSession, View};
use gpui_kit::{AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, IntoElement, Render, Task, Window};
use protocol::attachment::Attachment;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{AgentKind, AttachmentId, Seq, Session, SessionId, Subagent, ToolUseId};

use crate::attachments::Picture;
use crate::folds::Folds;
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::list::TranscriptList;
use crate::long_texts::LongTexts;
use crate::questions::QuestionPanel;
use crate::rows::SubagentCalls;

mod answers;
mod outline;
mod record;
mod sending;

pub(crate) use record::AtWork;
use record::tick;

/// Things the owner of a transcript has to deal with.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    /// Something failed that the user should hear about.
    Error(String),
    /// A prompt couldn't be sent, so what was typed and attached is handed back, to be offered again.
    NotSent { message: String, text: String, files: Vec<FileToSend> },
    /// The agent finished a turn, so anything derived from the worktree may be out of date.
    TurnEnded,
    /// The question in the composer's place changed: one came, or the one there was answered. `left` is the focus of
    /// the one that went, if one did, so focus that was in it can move to what's drawn now.
    Asking { left: Option<FocusHandle> },
    /// Someone asked to see the work of the subagent the call `call` started, in the subagent tab. `title` is what the
    /// call says it's for, for a subagent whose record hasn't come.
    OpenSubagent { call: ToolUseId, title: String },
}

pub(crate) struct Model {
    /// The agent the chat runs, as its record names it, which never changes.
    pub agent: AgentKind,
    pub folds: Folds,
    /// The images prompts carry, by attachment, from the first time a row drew each.
    pub pictures: HashMap<AttachmentId, Picture>,
    /// Where the calls that started subagents are, as rows have looked for them.
    pub subagent_calls: RefCell<SubagentCalls>,
    /// What the agent is at, as the session's record says, which the transcript's last row draws.
    pub at_work: AtWork,
    /// The long messages' text, read ahead of being drawn.
    pub long_texts: LongTexts,
    /// Each subagent's record, by the call that started it, as the hub keeps them. None from a hub that doesn't.
    pub subagents: HashMap<ToolUseId, Subagent>,
    /// The `seq` of the event each subagent's call was drawn in, so the row is measured again when its record
    /// changes. A call's row is measured only once it's been drawn, so one that isn't here needs nothing.
    pub subagent_rows: RefCell<HashMap<ToolUseId, Seq>>,
    /// In the subagent tab, the call that started the subagent it shows, alone.
    pub alone: Option<ToolUseId>,
}

impl Model {
    /// One that's drawn nothing yet, for a chat with `agent`, or for its subagent tab.
    fn with(agent: AgentKind, at_work: AtWork) -> Self {
        Model {
            agent,
            folds: Folds::default(),
            pictures: HashMap::new(),
            subagent_calls: RefCell::default(),
            at_work,
            long_texts: LongTexts::default(),
            subagents: HashMap::new(),
            subagent_rows: RefCell::default(),
            alone: None,
        }
    }
}

/// An idle chat with Claude, for tests that draw rows without one.
#[cfg(test)]
impl Default for Model {
    fn default() -> Self {
        Model::with(AgentKind::Claude, AtWork::default())
    }
}

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
    list: TranscriptList,
    _changes: Task<()>,
    /// Draws the time so far again while the agent works.
    ticking: Option<Task<()>>,
    prompts: outline::Prompts,
    /// The oldest question waiting on an answer, to be asked in the composer's place.
    asking: Option<Entity<QuestionPanel>>,
    /// Where every question waiting on an answer is in the log. The transcript draws them once they're answered.
    waiting: Vec<Seq>,
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
        let list = TranscriptList::new(cx);
        list.set_rows(rows, loaded);
        let linked = LinkedImages::new(client.clone(), session.id.clone(), cx.weak_entity());
        let ticking = at_work.working.then(|| tick(cx));
        let model = Rc::new(RefCell::new(Model::with(session.agent, at_work)));
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
            list,
            _changes: following,
            ticking,
            prompts: outline::Prompts::default(),
            asking: None,
            waiting: Vec::new(),
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
        // Asked for again the next time they're drawn, in case the connection was what failed.
        self.model.borrow_mut().pictures.retain(|_, picture| !matches!(picture, Picture::Unreadable));
        self.linked.reconnect(client);
    }

    /// The approvals waiting on an answer, newest last.
    pub fn open_approvals(&self) -> Vec<SessionEvent> {
        self.session.read(|log| log.open_approvals().into_iter().cloned().collect())
    }

    pub fn loaded_events(&self) -> usize {
        self.session.read(SessionLog::loaded_events)
    }

    /// How many rows are loaded as the session's log has them now. The list is told of them when the log says it has
    /// changed, which is after the last of the pages asked for together is in, so this can be ahead of the list.
    #[cfg(feature = "testing")]
    pub fn loaded_rows(&self) -> usize {
        self.session.read(|log| rows_and_loaded(log).1.iter().map(ExactSizeIterator::len).sum())
    }

    /// Whether every long text of the loaded events has been read. One is read on another thread once its row
    /// loads, and its row is measured again when that's done, so until then the session's height isn't settled.
    #[cfg(feature = "testing")]
    pub fn long_texts_read(&self, cx: &gpui_kit::App) -> bool {
        self.model.borrow().long_texts.all_read(cx)
    }

    /// The event at a row, if it's loaded.
    pub fn event_at(&self, row: usize) -> Option<SessionEvent> {
        self.session.read(|log| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded | Row::Pending(_) => None,
        })
    }

    /// Where the prompt recorded at a row stands with the agent, if that row is a recorded prompt that's loaded.
    pub fn prompt_state_at(&self, row: usize) -> Option<PromptState> {
        self.session.read(|log| match log.row(row) {
            Row::Event(event) if matches!(event.body, EventBody::UserPrompt { .. }) => {
                Some(log.prompt_state(event.seq))
            }
            Row::Event(_) | Row::Unloaded | Row::Pending(_) => None,
        })
    }

    /// Rows in the list, not counting the streaming tail.
    pub fn row_count(&self) -> usize {
        self.session.read(SessionLog::row_count)
    }

    pub fn streaming_text(&self) -> String {
        self.session.read(|log| log.tail().messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect())
    }

    /// Shows what changed in the session since this last looked, however much that is.
    fn catch_up(&mut self, cx: &mut Context<Self>) {
        let changes = self.session.take_changes();
        let errors = self.session.take_errors();
        let (rows, loaded) = self.session.read(rows_and_loaded);
        // A question opens and closes with an event, which is a row of its own rather than the tail.
        let events = self.session.read(SessionLog::row_count);
        let events_changed = changes.rows.as_ref().is_some_and(|changed| changed.start < events);
        self.list.set_rows(rows, loaded);
        self.read_ahead(changes.rows.clone().unwrap_or_default(), cx);
        if let Some(changed) = changes.rows {
            self.list.out_of_date(changed);
        }
        if events_changed {
            self.follow_question(cx);
        }
        for error in errors {
            cx.emit(TranscriptEvent::Error(error.to_string()));
        }
        if changes.turns_ended > 0 {
            cx.emit(TranscriptEvent::TurnEnded);
        }
        cx.notify();
    }

    /// Starts reading the long texts of rows whose content has changed, such as a page of history that's loaded,
    /// and of what the agent is still writing, so each has its height when the list measures it.
    fn read_ahead(&mut self, rows: std::ops::Range<usize>, cx: &mut Context<Self>) {
        let (this, mut m) = (cx.weak_entity(), self.model.borrow_mut());
        self.session.read(|log| m.long_texts.follow(log, rows, &this, cx));
    }

    /// Opens or closes a block drawn in the row of the event at `seq`, which changes that row's height.
    pub(crate) fn set_open(&mut self, seq: Seq, key: ItemKey, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, open);
        self.remeasure(seq, cx);
    }

    /// Measures the row of the event at `seq` again, since what it draws has changed height, and draws it.
    pub(crate) fn remeasure(&mut self, seq: Seq, cx: &mut Context<Self>) {
        // Not loaded, so not measured either.
        if let Some(row) = self.session.read(|log| log.row_of(seq)) {
            self.list.out_of_date(row..row + 1);
        }
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

    /// Fetches an image a row is drawing, and measures the row again once it's here, since an opened image is as tall
    /// as its shape makes it.
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
        let (model, session, linked) = (self.model.clone(), self.session.clone(), self.linked.clone());
        let (this, wanted) = (cx.weak_entity(), self.session.clone());
        // The list's last row is the tail, even while the log has rows it hasn't been told of, such as a log by place
        // that has just learned how far it goes, whose first row would otherwise be drawn there and fetched.
        let tail = self.list.row_count().saturating_sub(1);
        let rows = self.list.render(
            ElementId::Name(self.list_id.clone().into()),
            move |ix, _, cx| {
                let item = crate::rows::Item { ix, tail };
                crate::rows::render_row(&model, &session, &this, &linked, item, cx)
            },
            move |row| wanted.want(row),
            window,
            cx,
        );
        self.with_outline(rows.into_any_element(), window, cx)
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

/// What the user calls an approval mode.
pub fn mode_label(mode: &str) -> &str {
    match mode {
        "manual" | "default" => "Ask first",
        "plan" => "Plan",
        "acceptEdits" => "Accept edits",
        "bypassPermissions" => "Full access",
        other => other,
    }
}
