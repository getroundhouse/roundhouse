//! The transcript view: what it's showing, and how it answers.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use client_core::session_log::{PromptState, Row, SessionLog};
use client_core::{Client, FileToSend, OpenSession};
use gpui_kit::{AppContext, Context, ElementId, EventEmitter, IntoElement, Render, Task, Window};
use protocol::attachment::Attachment;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ApprovalId, AttachmentId, Seq, Session, SessionId, seq_index};

use crate::attachments::Picture;
use crate::folds::Folds;
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::long_texts::LongTexts;
use crate::rows::SubagentCalls;
use crate::scroller::Scroller;

mod answers;
mod outline;
mod record;

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
}

#[derive(Default)]
pub(crate) struct Model {
    pub folds: Folds,
    /// What's been chosen so far on questions with more than one thing to choose, by the approval's
    /// ID and then the question's text.
    pub picks: HashMap<ApprovalId, BTreeMap<String, Vec<String>>>,
    /// The images prompts carry, by attachment, from the first time a row drew each.
    pub pictures: HashMap<AttachmentId, Picture>,
    /// Where the calls that started subagents are, as rows have looked for them.
    pub subagent_calls: RefCell<SubagentCalls>,
    /// What the agent is at, as the session's record says, which the transcript's last row draws.
    pub at_work: AtWork,
    /// The long messages' text, read ahead of being drawn.
    pub long_texts: LongTexts,
}

pub struct Transcript {
    client: Client,
    session_id: SessionId,
    /// Client-core's copy of the session, which this only reads.
    session: OpenSession,
    model: Rc<RefCell<Model>>,
    /// The images the agent's text points to.
    linked: LinkedImages,
    scroller: Scroller,
    _changes: Task<()>,
    /// Draws the time so far again while the agent works.
    ticking: Option<Task<()>>,
    prompts: outline::Prompts,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        let open = OpenSession::start(&client, session);
        // The row after the log is the streaming tail, and it's always there. An empty one has no height.
        let rows = open.read(SessionLog::row_count) + 1;
        let scroller = Scroller::new(rows, cx);
        let linked = LinkedImages::new(client.clone(), session.id.clone(), cx.weak_entity());
        let at_work = AtWork::of(session);
        let ticking = at_work.working.then(|| tick(cx));
        let model = Rc::new(RefCell::new(Model { at_work, ..Model::default() }));
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
            session: open,
            model,
            linked,
            scroller,
            _changes: following,
            ticking,
            prompts: outline::Prompts::default(),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn scroller(&self) -> &Scroller {
        &self.scroller
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
        let rows = self.session.read(SessionLog::row_count) + 1;
        let have = self.scroller.item_count();
        self.scroller.set_rows(rows);
        self.read_ahead(changes.rows.clone().unwrap_or_default(), cx);
        if let Some(changed) = changes.rows {
            self.scroller.remeasure_items(changed);
        }
        // The last rows can change height as text streams into them.
        self.scroller.remeasure_items(have.min(rows).saturating_sub(2)..rows);
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
        let row = seq_index(seq) - 1;
        self.scroller.remeasure_items(row..row + 1);
        cx.notify();
    }

    /// Whether thinking shows open, here and as it streams, rather than as a line to open. A block someone opened
    /// or closed stays as they left it.
    pub fn set_thinking_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !self.model.borrow_mut().folds.set_thinking_open(open) {
            return;
        }
        // Any row can hold thinking, so every row can change height.
        self.scroller.remeasure();
        cx.notify();
    }

    /// Shows the prompt at once, and takes it back if it can't be sent. Returns false if it's empty.
    pub fn send_prompt(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        self.send_prompt_with(text, Vec::new(), cx)
    }

    /// Shows the prompt at once with what it carries, then adds each file to the session and sends the prompt. If any
    /// of that fails, it takes the prompt back and hands its text and files back with [`TranscriptEvent::NotSent`].
    /// Returns false if there's nothing to send.
    pub fn send_prompt_with(&mut self, text: String, files: Vec<FileToSend>, cx: &mut Context<Self>) -> bool {
        if text.trim().is_empty() && files.is_empty() {
            return false;
        }
        let client_msg_id = client_core::new_message_id();
        {
            let mut m = self.model.borrow_mut();
            for file in files.iter().filter(|file| file.attachment.media_type.is_image()) {
                let picture = Picture::read(file.attachment.media_type, file.data.clone());
                m.pictures.insert(file.attachment.id.clone(), picture);
            }
        }
        let carried = files.iter().map(|file| file.attachment.clone()).collect();
        self.session.add_pending(client_msg_id.clone(), text.clone(), carried);
        self.catch_up(cx);
        self.scroller.scroll_to_end();
        cx.notify();

        // Started on a thread of its own, so it finishes even if the app quits, and hands back what it was given.
        let (id, msg_id, typed) = (self.session_id.clone(), client_msg_id.clone(), text.clone());
        let sending = self.client.start(move |client| {
            let sent = client.send_prompt_carrying(&id, &text, Some(msg_id), &files);
            Ok((sent, text, files))
        });
        cx.spawn(async move |this, cx| {
            let (message, text, files) = match sending.recv().await {
                Ok((Ok(_), ..)) => return,
                Ok((Err(e), text, files)) => (not_sent(&e), text, files),
                // Only a call that never ran ends here, and its files went with it.
                Err(e) => (not_sent(&e), typed, Vec::new()),
            };
            let _ = this.update(cx, |t, cx| {
                t.session.remove_pending(&client_msg_id);
                t.catch_up(cx);
                cx.emit(TranscriptEvent::NotSent { message, text, files });
                cx.notify();
            });
        })
        .detach();
        true
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

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let stopping = self.client.start(move |client| client.session_cancel(&id));
        cx.spawn(async move |this, cx| {
            if let Err(e) = stopping.recv().await {
                let _ =
                    this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("couldn't stop the agent: {e}"))));
            }
        })
        .detach();
    }
}

impl Render for Transcript {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (model, session, linked) = (self.model.clone(), self.session.clone(), self.linked.clone());
        let this = cx.weak_entity();
        let rows = self.scroller.render(
            ElementId::Name(self.session_id.to_string().into()),
            move |ix, bar, _, cx| crate::rows::render_row(&model, &session, &this, &linked, bar, ix, cx),
            window,
            cx,
        );
        self.with_outline(rows.into_any_element(), window, cx)
    }
}

/// What to tell someone whose prompt didn't go. A hub from before attachments is the one failure they can fix.
fn not_sent(error: &client_core::ClientError) -> String {
    if error.is_unsupported() {
        "couldn't send that: the hub is older than the app, and can't take attachments until it's updated".to_string()
    } else {
        format!("couldn't send that: {error}")
    }
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
