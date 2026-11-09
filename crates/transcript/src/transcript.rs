//! The transcript view: what it's showing, and how it answers.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use client_core::session_log::{Row, SessionLog};
use client_core::{Client, OpenSession};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::{
    AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, Render, StyleRefinement, Styled, Task, Window,
    px,
};
use protocol::event::{ApprovalDecision, EventBody, SessionEvent};
use protocol::model::{ApprovalId, Seq, Session, SessionId, seq_index};

use crate::item_key::ItemKey;

/// Things the owner of a transcript has to deal with.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    /// Something failed that the user should hear about.
    Error(String),
    /// The agent finished a turn, so anything derived from the worktree may be out of date.
    TurnEnded,
}

pub(crate) struct Model {
    /// Blocks the user has opened.
    pub expanded: HashSet<ItemKey>,
    /// What's been chosen so far on questions with more than one thing to choose, by the approval's
    /// ID and then the question's text.
    pub picks: HashMap<ApprovalId, BTreeMap<String, Vec<String>>>,
}

pub struct Transcript {
    client: Client,
    session_id: SessionId,
    /// Client-core's copy of the session, which this only reads.
    session: OpenSession,
    model: Rc<RefCell<Model>>,
    scroller: Entity<MessageScrollerState>,
    _changes: Task<()>,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        let open = OpenSession::start(&client, session);
        // The row after the log is the streaming tail, and it's always there. An empty one has no height.
        let rows = open.read(SessionLog::row_count) + 1;
        let scroller = cx.new(|cx| MessageScrollerState::new(rows, cx));
        let model = Rc::new(RefCell::new(Model { expanded: HashSet::new(), picks: HashMap::new() }));
        let signals = open.changed().clone();
        let following = cx.spawn(async move |this, cx| {
            while signals.recv_async().await.is_ok() {
                if this.update(cx, Transcript::catch_up).is_err() {
                    return;
                }
            }
        });
        Self { client, session_id: session.id.clone(), session: open, model, scroller, _changes: following }
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
        self.scroller.update(cx, |list, cx| {
            let have = list.item_count();
            if rows > have {
                list.append(rows - have, cx);
            } else if rows < have {
                list.splice(rows..have, 0, cx);
            }
            if let Some(changed) = changes.rows {
                list.remeasure_items(changed.start.min(rows)..changed.end.min(rows), cx);
            }
            // The last rows can change height as text streams into them.
            list.remeasure_items(have.min(rows).saturating_sub(2)..rows, cx);
        });
        for error in errors {
            cx.emit(TranscriptEvent::Error(error.to_string()));
        }
        if changes.turns_ended > 0 {
            cx.emit(TranscriptEvent::TurnEnded);
        }
        cx.notify();
    }

    /// Records a choice on a question card, and answers once nothing else is needed.
    pub(crate) fn pick_answer(
        &mut self,
        seq: Seq,
        approval_id: ApprovalId,
        question: &crate::questions::Question,
        label: String,
        only_question: bool,
        cx: &mut Context<Self>,
    ) {
        if only_question && !question.multiple {
            let answers = std::iter::once((question.text.clone(), label)).collect();
            self.answer_approval(approval_id, ApprovalDecision::Answer { answers }, cx);
            return;
        }
        {
            let mut m = self.model.borrow_mut();
            let picks = m.picks.entry(approval_id).or_default().entry(question.text.clone()).or_default();
            crate::questions::toggle_pick(picks, label, question.multiple);
        }
        let row = seq_index(seq) - 1;
        self.scroller.update(cx, |list, cx| list.remeasure_items(row..row + 1, cx));
        cx.notify();
    }

    /// Sends what's been chosen on a card with more than one thing to choose.
    pub(crate) fn send_answers(&mut self, approval_id: ApprovalId, cx: &mut Context<Self>) {
        let picks = self.model.borrow_mut().picks.remove(&approval_id).unwrap_or_default();
        self.answer_approval(
            approval_id,
            ApprovalDecision::Answer { answers: crate::questions::answers_from(&picks) },
            cx,
        );
    }

    /// Takes text typed in the composer as the answer to an open question, if there is one: the first
    /// question on it without an answer yet. Returns false if there's nothing to answer.
    pub fn answer_with_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let open = self.open_approvals().into_iter().find_map(|event| {
            let EventBody::ApprovalRequested { approval_id, tool_name, input, .. } = event.body else {
                return None;
            };
            (tool_name == crate::questions::TOOL_NAME)
                .then(|| (event.seq, approval_id, crate::questions::parse(&input)))
        });
        let Some((seq, approval_id, asked)) = open else { return false };
        let done = {
            let mut m = self.model.borrow_mut();
            let picks = m.picks.entry(approval_id.clone()).or_default();
            let Some(unanswered) = crate::questions::first_unanswered(&asked, picks) else { return false };
            picks.insert(unanswered.text.clone(), vec![text.to_string()]);
            crate::questions::first_unanswered(&asked, picks).is_none()
        };
        if done {
            self.send_answers(approval_id, cx);
        } else {
            let row = seq_index(seq) - 1;
            self.scroller.update(cx, |list, cx| list.remeasure_items(row..row + 1, cx));
            cx.notify();
        }
        true
    }

    /// Opens or closes a block drawn in the row of the event at `seq`, which changes that row's height.
    pub(crate) fn toggle(&mut self, seq: Seq, key: ItemKey, cx: &mut Context<Self>) {
        {
            let mut m = self.model.borrow_mut();
            if !m.expanded.remove(&key) {
                m.expanded.insert(key);
            }
        }
        let row = seq_index(seq) - 1;
        self.scroller.update(cx, |list, cx| {
            list.remeasure_items(row..row + 1, cx);
        });
        cx.notify();
    }

    /// Shows the prompt at once, and takes it back if it can't be sent. Returns false if it's empty.
    /// What someone typed: the answer to the question the agent is waiting on, if it's waiting on one, and
    /// otherwise a prompt. Whether it was taken.
    pub fn submit(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        self.answer_with_text(&text, cx) || self.send_prompt(text, cx)
    }

    pub fn send_prompt(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let client_msg_id = client_core::new_message_id();
        self.session.add_pending(client_msg_id.clone(), text.clone());
        self.catch_up(cx);
        self.scroller.update(cx, MessageScrollerState::scroll_to_end);
        cx.notify();

        let (client, id, msg_id) = (self.client.clone(), self.session_id.clone(), client_msg_id.clone());
        cx.spawn(async move |this, cx| {
            let sent = cx.background_spawn(async move { client.session_prompt(&id, &text, Some(msg_id)) }).await;
            if let Err(e) = sent {
                let _ = this.update(cx, |t, cx| {
                    t.session.remove_pending(&client_msg_id);
                    t.catch_up(cx);
                    cx.emit(TranscriptEvent::Error(format!("couldn't send that: {e}")));
                    cx.notify();
                });
            }
        })
        .detach();
        true
    }

    pub fn answer_approval(&mut self, approval_id: ApprovalId, decision: ApprovalDecision, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.session_id.clone());
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { client.session_approve(&id, &approval_id, decision) }).await;
            if let Err(e) = result {
                let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("couldn't answer that: {e}"))));
            }
        })
        .detach();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.session_id.clone());
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { client.session_cancel(&id) }).await;
            if let Err(e) = result {
                let _ =
                    this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("couldn't stop the agent: {e}"))));
            }
        })
        .detach();
    }
}

impl Render for Transcript {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (model, session) = (self.model.clone(), self.session.clone());
        let this = cx.weak_entity();
        MessageScroller::new(
            ElementId::Name(self.session_id.to_string().into()),
            self.scroller.clone(),
            move |ix, _, cx| crate::rows::render_row(&model, &session, &this, ix, cx),
        )
        // Spacing is the rows' own. Most log events show nothing, and those mustn't leave gaps.
        .with_row_style(StyleRefinement::default().pb(px(0.)))
        .with_jump_button_label("Jump to latest")
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
