//! The transcript view: what it's showing, and how it answers.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use client_core::session_log::{PromptState, Row, SessionLog};
use client_core::{Client, FileToSend, OpenSession};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::{
    AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, Render, StyleRefinement, Styled, Task, Window,
    px,
};
use protocol::attachment::Attachment;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ApprovalId, AttachmentId, Seq, Session, SessionId, seq_index};
use protocol::tail::Tail;

use crate::attachments::Picture;
use crate::folds::Folds;
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::placement::{LogRow, Placement};

mod answers;
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
    /// Where each of the log's rows is drawn in the list, as of the last time this caught up with the log.
    pub placement: Placement,
    /// What the agent is writing, from the same moment as the placement. The log moves what it wrote into a row of its
    /// own as soon as it's recorded, and the list has no place for that row until it catches up, so drawing the log's
    /// tail would leave it out for a frame.
    pub streaming: Tail,
    /// What the agent is at, as the session's record says, which the transcript's last row draws.
    pub at_work: AtWork,
}

pub struct Transcript {
    client: Client,
    session_id: SessionId,
    /// Client-core's copy of the session, which this only reads.
    session: OpenSession,
    model: Rc<RefCell<Model>>,
    /// The images the agent's text points to.
    linked: LinkedImages,
    scroller: Entity<MessageScrollerState>,
    _changes: Task<()>,
    /// Draws the time so far again while the agent works.
    ticking: Option<Task<()>>,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        let open = OpenSession::start(&client, session);
        let (placement, streaming) = open.read(|log| (Placement::of(log, &Folds::default()), log.tail().clone()));
        // The row after the log is the streaming tail, and it's always there. An empty one has no height.
        let rows = placement.len() + 1;
        let scroller = cx.new(|cx| MessageScrollerState::new(rows, cx));
        let linked = LinkedImages::new(client.clone(), session.workspace_id.clone(), cx.weak_entity());
        let at_work = AtWork::of(session);
        let ticking = at_work.working.then(|| tick(cx));
        let model = Rc::new(RefCell::new(Model { placement, streaming, at_work, ..Model::default() }));
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
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn scroller(&self) -> &Entity<MessageScrollerState> {
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

    /// The event at a row of the log, if it's loaded.
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

    /// Rows in the log, which is one for each event and each prompt still being sent. The list draws them, except a
    /// subagent's work under a closed call, and then the streaming tail.
    pub fn row_count(&self) -> usize {
        self.session.read(SessionLog::row_count)
    }

    /// What the transcript draws of what the agent is writing, as of the last time it caught up with the log.
    pub fn streaming_text(&self) -> String {
        let m = self.model.borrow();
        m.streaming.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }

    /// Shows what changed in the session since this last looked, however much that is.
    fn catch_up(&mut self, cx: &mut Context<Self>) {
        let changes = self.session.take_changes();
        let errors = self.session.take_errors();
        self.place(cx);
        let (remeasured, tail) = {
            let m = self.model.borrow();
            let rows = self.session.read(SessionLog::row_count);
            // The rows that changed, and the last ones, which can change height as text streams into them.
            let last = rows.saturating_sub(2)..rows;
            let touched = changes.rows.into_iter().flatten().chain(last).map(LogRow);
            (m.placement.positions_of(touched), m.placement.tail())
        };
        self.scroller.update(cx, |list, cx| {
            for run in remeasured {
                list.remeasure_items(run.start.0..run.end.0, cx);
            }
            list.remeasure_items(tail.0..tail.0 + 1, cx);
        });
        for error in errors {
            cx.emit(TranscriptEvent::Error(error.to_string()));
        }
        if changes.turns_ended > 0 {
            cx.emit(TranscriptEvent::TurnEnded);
        }
        cx.notify();
    }

    /// Puts the list's rows where the log and what's open say they go, moving only the ones that have to, and measures
    /// again the ones that look different because of where others went.
    fn place(&mut self, cx: &mut Context<Self>) {
        let (update, rows) = {
            let m = &mut *self.model.borrow_mut();
            let update = self.session.read(|log| {
                m.streaming.clone_from(log.tail());
                m.placement.follow(log, &m.folds)
            });
            // The rows of the log, and the streaming tail after them.
            (update, m.placement.len() + 1)
        };
        self.scroller.update(cx, |list, cx| {
            for splice in update.splices {
                let at = splice.at.0;
                list.splice(at..at + splice.removed, splice.added, cx);
            }
            if list.item_count() != rows {
                debug_assert_eq!(list.item_count(), rows, "the list has a row for each the placement draws");
                // Every row is measured again, and the list scrolls to the end, but it draws the right rows.
                list.reset(rows, cx);
            }
            for run in update.restyled {
                list.remeasure_items(run.start.0..run.end.0, cx);
            }
        });
    }

    /// Opens or closes a block drawn in the row of the event at `seq`, which changes that row's height. Opening a call
    /// that started a subagent puts the subagent's work in rows after it, and closing it takes them out.
    pub(crate) fn set_open(&mut self, seq: Seq, key: ItemKey, open: bool, cx: &mut Context<Self>) {
        self.model.borrow_mut().folds.choose(key, open);
        self.place(cx);
        self.remeasure(seq, cx);
    }

    /// Measures the row of the event at `seq` again, since what it draws has changed height, and draws it. A subagent's
    /// work is drawn in rows of its own after its call, and changes height with it.
    pub(crate) fn remeasure(&mut self, seq: Seq, cx: &mut Context<Self>) {
        let positions = self.model.borrow().placement.drawn_with(LogRow(seq_index(seq) - 1));
        if let Some(positions) = positions {
            self.scroller.update(cx, |list, cx| {
                list.remeasure_items(positions.start.0..positions.end.0, cx);
            });
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
        self.scroller.update(cx, MessageScrollerState::remeasure);
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
        self.scroller.update(cx, MessageScrollerState::scroll_to_end);
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (model, session, linked) = (self.model.clone(), self.session.clone(), self.linked.clone());
        let this = cx.weak_entity();
        MessageScroller::new(
            ElementId::Name(self.session_id.to_string().into()),
            self.scroller.clone(),
            move |ix, _, cx| crate::rows::render_row(&model, &session, &this, &linked, ix, cx),
        )
        // Spacing is the rows' own. Most log events show nothing, and those mustn't leave gaps.
        .with_row_style(StyleRefinement::default().pb(px(0.)))
        .with_jump_button_label("Jump to latest")
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
