//! What the agent is waiting on the person for, asked in the composer's place: its questions, one at a time, each
//! option a numbered row, a row to type another answer into, and under them a dot for each question and the arrow that
//! sends. A request to use a tool is asked the same way, as one question that's allowed or denied. [`Asks`] says which
//! it is, and each way they differ.

use std::time::{Duration, Instant};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement, Render,
    Styled, WeakEntity, Window, div,
};
use look::{InRegion as _, Off as _, Region};
use protocol::event::{ApprovalDecision, Question, QuestionAnswer};
use protocol::model::{ApprovalId, ToolUseId};

use super::choice::{Choice, next_unanswered};
use crate::transcript::Transcript;

mod asks;
mod heading;
mod kept;
mod options;
mod pager;
mod selectable;
mod tool_use;
mod typing;

pub(crate) use asks::Asks;
use typing::Other;

/// How long a question that's just come ignores its keys, and goes on ignoring them while they keep coming. It's longer
/// than the gap between keys while typing, so the end of something typed in the composer as the question came can't
/// answer it, and shorter than it takes to read a question.
pub(crate) const SETTLE: Duration = Duration::from_millis(500);

pub struct QuestionPanel {
    transcript: WeakEntity<Transcript>,
    approval_id: ApprovalId,
    /// What it asks, which says what it shows and how an answer is sent.
    asks: Asks,
    /// The subagent that asks, as the call that started it, or `None` for the agent itself.
    asked_by: Option<ToolUseId>,
    /// What that subagent is for, as its record says, which the transcript passes on as the record comes or changes.
    asker: Option<String>,
    /// How many more requests wait behind this one, to be asked in turn.
    more_waiting: usize,
    asked: Vec<Asked>,
    showing: usize,
    focus: FocusHandle,
    /// Keys before this are taken as typing that was meant for the composer, and each one pushes it back. Choosing
    /// anything ends it, since that's the person turning to the question.
    quiet_until: Instant,
    /// It has answered, so the arrow and the keys wait for the hub rather than answering again.
    answered: bool,
}

/// One of the questions, and what's been chosen on it.
struct Asked {
    question: Question,
    choice: Choice,
    /// Its row for typing another answer, made the first time the question shows, since an input needs the window.
    other: Option<Other>,
}

impl Asked {
    fn typed(&self, cx: &App) -> String {
        self.other.as_ref().map(|other| other.input.read(cx).text().to_string()).unwrap_or_default()
    }

    fn answer(&self, cx: &App) -> Option<QuestionAnswer> {
        self.choice.answer(&self.typed(cx))
    }
}

impl QuestionPanel {
    /// Asks what `asks` says, for the subagent `asked_by` started, or for the agent itself.
    pub(crate) fn new(
        transcript: WeakEntity<Transcript>,
        approval_id: ApprovalId,
        asks: Asks,
        asked_by: Option<ToolUseId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let questions = asks.questions().into_iter();
        let asked = questions.map(|question| Asked { question, choice: Choice::default(), other: None }).collect();
        Self {
            transcript,
            approval_id,
            asks,
            asked_by,
            asker: None,
            more_waiting: 0,
            asked,
            showing: 0,
            focus: cx.focus_handle(),
            quiet_until: cx.background_executor().now() + SETTLE,
            answered: false,
        }
    }

    pub fn approval_id(&self) -> &ApprovalId {
        &self.approval_id
    }

    /// The subagent that asks, as the call that started it.
    pub(crate) fn asked_by(&self) -> Option<&ToolUseId> {
        self.asked_by.as_ref()
    }

    /// Says what's around the request: what the subagent that asks is for, and how many more requests wait behind
    /// this one.
    pub(crate) fn set_around(&mut self, asker: Option<String>, more_waiting: usize, cx: &mut Context<Self>) {
        if self.asker != asker || self.more_waiting != more_waiting {
            (self.asker, self.more_waiting) = (asker, more_waiting);
            cx.notify();
        }
    }

    /// The hub didn't take the answer, so it can be given again.
    pub(crate) fn not_answered(&mut self, cx: &mut Context<Self>) {
        self.answered = false;
        cx.notify();
    }

    /// Each question's answer, if it has one yet.
    fn answers_so_far(&self, cx: &App) -> Vec<Option<QuestionAnswer>> {
        self.asked.iter().map(|asked| asked.answer(cx)).collect()
    }

    /// Chooses an option on the question showing. With one answer allowed, that answers it, so the next question
    /// without an answer shows.
    fn pick(&mut self, option: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asked) = self.asked.get_mut(self.showing) else { return };
        if option >= asked.question.options.len() {
            return;
        }
        let multiple = asked.question.multiple;
        asked.choice.pick(option, multiple);
        self.answering(true, cx);
        let next = if multiple { None } else { next_unanswered(&self.answers_so_far(cx), self.showing) };
        self.show(next.unwrap_or(self.showing), window, cx);
    }

    /// Where several answers are allowed, takes back what's typed, or chooses it again.
    fn toggle_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asked) = self.asked.get_mut(self.showing) else { return };
        let has_text = !asked.typed(cx).trim().is_empty();
        asked.choice.toggle_typed(has_text);
        self.answering(true, cx);
        self.show(self.showing, window, cx);
    }

    fn show(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) {
        if q < self.asked.len() {
            self.showing = q;
        }
        self.quiet_until = cx.background_executor().now();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(q) = self.showing.checked_add_signed(by) {
            self.show(q, window, cx);
        }
    }

    /// What Enter does: shows the next question without an answer, and once they all have one, sends them.
    fn go_on(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match next_unanswered(&self.answers_so_far(cx), self.showing) {
            Some(q) => self.show(q, window, cx),
            None => self.send(cx),
        }
    }

    /// Sends the answers there are. A question left without one is sent with nothing in it, so the agent can tell.
    fn send(&mut self, cx: &mut Context<Self>) {
        let answered = self.answers_so_far(cx);
        if self.answered || answered.iter().all(Option::is_none) {
            return;
        }
        let answers = answered.into_iter().map(Option::unwrap_or_default).collect();
        if let Some(decision) = self.asks.decision(answers) {
            self.answer(decision, cx);
        }
    }

    fn skip(&mut self, cx: &mut Context<Self>) {
        if let Some(skipped) = self.asks.skipped().filter(|_| !self.answered) {
            self.answer(skipped, cx);
        }
    }

    fn answer(&mut self, decision: ApprovalDecision, cx: &mut Context<Self>) {
        self.answered = true;
        let approval_id = self.approval_id.clone();
        let _ = self.transcript.update(cx, |t, cx| t.answer_approval(approval_id, decision, cx));
        // The hub lets go of what it kept as the question is answered, so it's still there if the answer can't be sent.
        cx.notify();
    }

    /// The keys work while the panel has focus rather than its typing row: a number chooses that option, 0 goes to the
    /// typing row, the arrows move between questions, and Enter goes on. Until the question settles, they do nothing.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if !self.focus.is_focused(window) || keystroke.modifiers.modified() {
            return;
        }
        let now = cx.background_executor().now();
        if now < self.quiet_until {
            self.quiet_until = now + SETTLE;
            cx.stop_propagation();
            return;
        }
        match keystroke.key.as_str() {
            "enter" => self.go_on(window, cx),
            "left" => self.step(-1, window, cx),
            "right" => self.step(1, window, cx),
            "0" => self.type_other(window, cx),
            key => {
                let rows = self.asked.get(self.showing).map_or(0, |asked| asked.question.options.len());
                match key.parse::<usize>() {
                    Ok(number @ 1..=9) if number <= rows => self.pick(number - 1, window, cx),
                    // A number with no row chooses nothing, and is still the panel's key.
                    Ok(1..=9) => self.typed_past(cx),
                    // A key that types a character is one character long, or the space bar. Tab and the like aren't.
                    _ if key.chars().count() == 1 || key == "space" => return self.typed_past(cx),
                    _ => return,
                }
            }
        }
        cx.stop_propagation();
    }

    /// A key that types something and isn't one of the panel's, which is someone typing who may not have seen what's
    /// asked. Where [`Asks`] says, it takes back the option that's chosen, and the panel goes quiet again as it does
    /// when it's just come. What's typed in the typing row stays chosen, since that was typed here.
    fn typed_past(&mut self, cx: &mut Context<Self>) {
        if !self.asks.typing_takes_back_what_is_chosen() {
            return;
        }
        if let Some(asked) = self.asked.get_mut(self.showing) {
            asked.choice.take_back_options();
        }
        self.quiet_until = cx.background_executor().now() + SETTLE;
        self.answering(true, cx);
        cx.notify();
    }
}

impl Focusable for QuestionPanel {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for QuestionPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edge = look::colors(cx).edge;
        let answered = self.answers_so_far(cx);
        let ready = !self.answered && answered.iter().any(Option::is_some);
        let q = self.showing;
        let question = self.asked.get(q).map(|asked| asked.question.clone());
        let heading = heading::heading(self, question.as_ref(), cx);
        let detail = self.asks.detail(cx);
        let pager = pager::pager(self, &answered, cx).or_else(|| pager::more_waiting(self.more_waiting, cx));
        let options = question.map(|question| {
            let other = self.other(q, window, cx);
            options::list(self, &question, &other, cx)
        });
        // The composer's Send, since this is in the composer's place.
        let send = Button::new("question-send")
            .primary()
            .small()
            .icon(IconName::ArrowUp)
            .tooltip(if self.asked.len() > 1 { "Send answers" } else { "Send answer" })
            .off_while(!ready)
            .on_click(cx.listener(|this, _, _, cx| this.send(cx)));
        v_flex()
            .region(Region::Question)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .w_full()
            .p_3()
            .gap_3()
            // The composer's corners, and the thin edge around the pane's own surface rather than a card's amber, since
            // being in the composer's place already says it's your turn.
            .rounded_lg()
            .border_1()
            .border_color(edge)
            .child(heading)
            .children(detail)
            .children(options)
            .child(h_flex().w_full().items_center().gap_2().child(div().flex_1().min_w_0().children(pager)).child(send))
    }
}
