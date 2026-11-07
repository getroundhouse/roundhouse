//! One session's transcript: its history by range and its live output, on the kit's `MessageScroller`
//! and `TextView`. `FINDINGS.md` has why it's built on those. Everything GPUI about a transcript is in
//! here, and the data is in `client_core::session_log`, which knows nothing about rendering.

mod log_block;
mod questions;
mod rows;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use client_core::session_log::{PAGE_EVENTS, SessionLog};
use client_core::{Client, SubscriptionItem};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::*;
use protocol::event::{ApprovalDecision, EventBody, SessionEvent};
use protocol::model::{Seq, Session};

/// Things the owner of a transcript has to deal with.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    /// Something failed that the user should hear about.
    Error(String),
    /// The connection to the hub has gone, so this transcript stopped updating.
    Disconnected,
    /// The agent finished a turn, so anything derived from the worktree may be out of date.
    TurnEnded,
}

pub(crate) struct Model {
    pub log: SessionLog,
    /// Blocks the user has opened, by the event's `seq` and the block's position in it.
    pub expanded: HashSet<(Seq, usize)>,
    /// What's been chosen so far on questions with more than one thing to choose, by the approval's
    /// ID and then the question's text.
    pub picks: HashMap<String, BTreeMap<String, Vec<String>>>,
    /// The row a page was last wanted for, which decides what stays loaded.
    pub focus: usize,
    pub fetch_scheduled: bool,
}

pub struct Transcript {
    client: Client,
    session_id: SharedString,
    pub(crate) model: Rc<RefCell<Model>>,
    scroller: Entity<MessageScrollerState>,
    _items: Task<()>,
}

impl EventEmitter<TranscriptEvent> for Transcript {}

impl Transcript {
    pub fn new(client: Client, session: &Session, cx: &mut Context<Self>) -> Self {
        let log = SessionLog::new(session.last_seq);
        // The row after the log is the streaming tail, and it's always there. An empty one has no height.
        let rows = log.row_count() + 1;
        let scroller = cx.new(|cx| MessageScrollerState::new(rows, cx));
        let model = Rc::new(RefCell::new(Model {
            log,
            expanded: HashSet::new(),
            picks: HashMap::new(),
            focus: rows.saturating_sub(1),
            fetch_scheduled: false,
        }));

        // Starts a page back rather than at the tail: an open approval is always near the tail, and
        // this way it arrives without a separate request.
        let from = session.last_seq.saturating_sub(PAGE_EVENTS - 1).max(1);
        let (subscribing, id) = (client.clone(), session.id.clone());
        let items = cx.spawn(async move |this, cx| {
            let subscription = cx.background_spawn(async move { subscribing.session_subscribe(&id, from) }).await;
            let subscription = match subscription {
                Ok(subscription) => subscription,
                Err(e) => {
                    let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("{e}"))));
                    return;
                }
            };
            while let Ok(item) = subscription.items.recv_async().await {
                let done = matches!(item, SubscriptionItem::Disconnected);
                if this.update(cx, |t, cx| t.apply(item, cx)).is_err() || done {
                    return;
                }
            }
        });

        Self { client, session_id: session.id.clone().into(), model, scroller, _items: items }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn scroller(&self) -> &Entity<MessageScrollerState> {
        &self.scroller
    }

    /// The approvals waiting on an answer, newest last.
    pub fn open_approvals(&self) -> Vec<SessionEvent> {
        self.model.borrow().log.open_approvals().into_iter().cloned().collect()
    }

    pub fn loaded_events(&self) -> usize {
        self.model.borrow().log.loaded_events()
    }

    /// The event at a row, if it's loaded.
    pub fn event_at(&self, row: usize) -> Option<SessionEvent> {
        match self.model.borrow().log.row(row) {
            client_core::session_log::Row::Event(event) => Some(event.clone()),
            _ => None,
        }
    }

    /// Rows in the list, not counting the streaming tail.
    pub fn row_count(&self) -> usize {
        self.model.borrow().log.row_count()
    }

    pub fn streaming_text(&self) -> String {
        let model = self.model.borrow();
        model.log.tail().messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }

    fn apply(&mut self, item: SubscriptionItem, cx: &mut Context<Self>) {
        let mut turn_ended = false;
        match item {
            SubscriptionItem::Event(event) => {
                turn_ended = matches!(event.body, EventBody::TurnEnded { .. });
                self.model.borrow_mut().log.apply_event(event);
            }
            SubscriptionItem::Tail(tail) => self.model.borrow_mut().log.apply_tail(tail),
            SubscriptionItem::Delta(delta) => self.model.borrow_mut().log.apply_delta(&delta),
            SubscriptionItem::Notice(_) => {}
            SubscriptionItem::Disconnected => {
                cx.emit(TranscriptEvent::Disconnected);
                return;
            }
        }
        self.sync_rows(cx);
        if turn_ended {
            cx.emit(TranscriptEvent::TurnEnded);
        }
        cx.notify();
    }

    /// Tells the list about new rows, and that the last ones may have changed height.
    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        let want = self.model.borrow().log.row_count() + 1;
        self.scroller.update(cx, |list, cx| {
            let have = list.item_count();
            if want > have {
                list.append(want - have, cx);
            } else if want < have {
                list.splice(want..have, 0, cx);
            }
            let from = have.min(want).saturating_sub(2);
            list.remeasure_items(from..want, cx);
        });
    }

    /// Called from row rendering when an unloaded row comes into view.
    pub(crate) fn fetch_wanted(&mut self, cx: &mut Context<Self>) {
        let pages = {
            let mut m = self.model.borrow_mut();
            m.fetch_scheduled = false;
            let focus = m.focus;
            m.log.pages_to_fetch(focus)
        };
        for range in pages {
            let (client, id) = (self.client.clone(), self.session_id.clone());
            let start = range.start;
            cx.spawn(async move |this, cx| {
                let read = cx.background_spawn(async move {
                    // A size limit can cut a read short, so keep going until the page is complete.
                    let mut events = Vec::new();
                    let mut from = range.start;
                    while from < range.end {
                        let (page, _) = client.session_read(&id, from, (range.end - from) as u32, false)?;
                        let Some(last) = page.last().map(|e| e.seq) else { break };
                        events.extend(page);
                        from = last + 1;
                    }
                    Ok::<_, client_core::ClientError>(events)
                });
                let result = read.await;
                let _ = this.update(cx, |t, cx| t.install(start, result, cx));
            })
            .detach();
        }
    }

    fn install(
        &mut self,
        start: Seq,
        result: Result<Vec<SessionEvent>, client_core::ClientError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(events) => {
                let filled = {
                    let mut m = self.model.borrow_mut();
                    let focus = m.focus;
                    m.log.install_page(start, events, focus)
                };
                self.scroller.update(cx, |list, cx| {
                    list.remeasure_items(filled, cx);
                });
                cx.notify();
            }
            Err(e) => {
                self.model.borrow_mut().log.page_failed(start);
                cx.emit(TranscriptEvent::Error(format!("couldn't load history: {e}")));
            }
        }
    }

    /// Opens or closes a thinking or tool block.
    /// Records a choice on a question card, and answers once nothing else is needed.
    pub(crate) fn pick_answer(
        &mut self,
        seq: Seq,
        approval_id: String,
        question: &questions::Question,
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
            questions::toggle_pick(picks, label, question.multiple);
        }
        let row = seq as usize - 1;
        self.scroller.update(cx, |list, cx| list.remeasure_items(row..row + 1, cx));
        cx.notify();
    }

    /// Sends what's been chosen on a card with more than one thing to choose.
    pub(crate) fn send_answers(&mut self, approval_id: String, cx: &mut Context<Self>) {
        let picks = self.model.borrow_mut().picks.remove(&approval_id).unwrap_or_default();
        self.answer_approval(approval_id, ApprovalDecision::Answer { answers: questions::answers_from(&picks) }, cx);
    }

    /// Takes text typed in the composer as the answer to an open question, if there is one: the first
    /// question on it without an answer yet. Returns false if there's nothing to answer.
    pub fn answer_with_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let open = self.open_approvals().into_iter().find_map(|event| match event.body {
            EventBody::ApprovalRequested { approval_id, tool_name, input, .. } if tool_name == questions::TOOL_NAME => {
                Some((event.seq, approval_id, questions::parse(&input)))
            }
            _ => None,
        });
        let Some((seq, approval_id, asked)) = open else { return false };
        let done = {
            let mut m = self.model.borrow_mut();
            let picks = m.picks.entry(approval_id.clone()).or_default();
            let Some(unanswered) = questions::first_unanswered(&asked, picks) else { return false };
            picks.insert(unanswered.text.clone(), vec![text.to_string()]);
            questions::first_unanswered(&asked, picks).is_none()
        };
        if done {
            self.send_answers(approval_id, cx);
        } else {
            let row = seq as usize - 1;
            self.scroller.update(cx, |list, cx| list.remeasure_items(row..row + 1, cx));
            cx.notify();
        }
        true
    }

    pub(crate) fn toggle(&mut self, seq: Seq, block: usize, cx: &mut Context<Self>) {
        {
            let mut m = self.model.borrow_mut();
            if !m.expanded.remove(&(seq, block)) {
                m.expanded.insert((seq, block));
            }
        }
        let row = seq as usize - 1;
        self.scroller.update(cx, |list, cx| {
            list.remeasure_items(row..row + 1, cx);
        });
        cx.notify();
    }

    /// Shows the prompt at once, and takes it back if it can't be sent. Returns false if it's empty.
    pub fn send_prompt(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let client_msg_id = uuid::Uuid::new_v4().to_string();
        self.model.borrow_mut().log.add_pending(client_msg_id.clone(), text.clone());
        self.sync_rows(cx);
        self.scroller.update(cx, |list, cx| list.scroll_to_end(cx));
        cx.notify();

        let (client, id, msg_id) = (self.client.clone(), self.session_id.clone(), client_msg_id.clone());
        cx.spawn(async move |this, cx| {
            let sent = cx.background_spawn(async move { client.session_prompt(&id, &text, Some(msg_id)) }).await;
            if let Err(e) = sent {
                let _ = this.update(cx, |t, cx| {
                    t.model.borrow_mut().log.remove_pending(&client_msg_id);
                    t.sync_rows(cx);
                    cx.emit(TranscriptEvent::Error(format!("couldn't send that: {e}")));
                    cx.notify();
                });
            }
        })
        .detach();
        true
    }

    pub fn answer_approval(&mut self, approval_id: String, decision: ApprovalDecision, cx: &mut Context<Self>) {
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
        let model = self.model.clone();
        let this = cx.weak_entity();
        MessageScroller::new(ElementId::Name(self.session_id.clone()), self.scroller.clone(), move |ix, _, cx| {
            rows::render_row(&model, &this, ix, cx)
        })
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
