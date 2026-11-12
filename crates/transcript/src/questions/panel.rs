//! A question waiting on an answer, asked in the composer's place: one question at a time, each option a numbered row,
//! a row to type another answer into, and under them a dot for each question and the arrow that sends.

use std::time::{Duration, Instant};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement, Render, Styled, Subscription, WeakEntity, Window, div, px,
};
use look::{Hand as _, InRegion as _, Off as _, Region};
use protocol::event::ApprovalDecision;
use protocol::model::{AnswerDraft, AnswerSoFar, ApprovalId};

use super::choice::{Choice, answers, next_unanswered};
use super::{Question, SKIPPED};
use crate::transcript::{Transcript, TranscriptEvent};

mod options;
mod pager;

/// How long a question that's just come ignores its keys, and goes on ignoring them while they keep coming. It's longer
/// than the gap between keys while typing, so the end of something typed in the composer as the question came can't
/// answer it, and shorter than it takes to read a question.
pub(crate) const SETTLE: Duration = Duration::from_millis(500);

pub struct QuestionPanel {
    transcript: WeakEntity<Transcript>,
    approval_id: ApprovalId,
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

struct Other {
    input: Entity<TextareaState>,
    _events: Subscription,
}

impl Asked {
    fn typed(&self, cx: &App) -> String {
        self.other.as_ref().map(|other| other.input.read(cx).text().to_string()).unwrap_or_default()
    }

    fn answer(&self, cx: &App) -> Option<String> {
        self.choice.answer(&self.question, &self.typed(cx))
    }
}

impl QuestionPanel {
    pub(crate) fn new(
        transcript: WeakEntity<Transcript>,
        approval_id: ApprovalId,
        asked: Vec<Question>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            transcript,
            approval_id,
            asked: asked
                .into_iter()
                .map(|question| Asked { question, choice: Choice::default(), other: None })
                .collect(),
            showing: 0,
            focus: cx.focus_handle(),
            quiet_until: cx.background_executor().now() + SETTLE,
            answered: false,
        }
    }

    pub fn approval_id(&self) -> &ApprovalId {
        &self.approval_id
    }

    /// The hub didn't take the answer, so it can be given again.
    pub(crate) fn not_answered(&mut self, cx: &mut Context<Self>) {
        self.answered = false;
        cx.notify();
    }

    /// Something's typed in a row for another answer, which would be lost with the question.
    pub fn has_typed(&self, cx: &App) -> bool {
        self.asked.iter().any(|asked| !asked.typed(cx).trim().is_empty())
    }

    /// What's chosen and typed so far, as the hub keeps it, or `None` with nothing, or once it's answered.
    pub fn so_far(&self, cx: &App) -> Option<AnswerDraft> {
        if self.answered {
            return None;
        }
        let answers: Vec<AnswerSoFar> = self
            .asked
            .iter()
            .map(|asked| {
                let (picked, other) = asked.choice.kept();
                let picked = picked.iter().map(|&option| u32::try_from(option).unwrap_or(u32::MAX)).collect();
                AnswerSoFar { picked, typed: asked.typed(cx), other }
            })
            .collect();
        let anything = answers.iter().any(|so_far| *so_far != AnswerSoFar::default());
        anything.then(|| AnswerDraft { approval_id: self.approval_id.clone(), answers })
    }

    /// Takes what was chosen and typed as the hub kept it, from before the app last stopped or from another client, in
    /// place of what's here, or clears it for `None`. The hub has it already, so it isn't kept again, and the focus
    /// stays where it is.
    pub fn take_so_far(&mut self, kept: Option<&AnswerDraft>, window: &mut Window, cx: &mut Context<Self>) {
        for q in 0..self.asked.len() {
            let so_far = kept.and_then(|kept| kept.answers.get(q)).cloned().unwrap_or_default();
            let Some(asked) = self.asked.get_mut(q) else { continue };
            let picked = so_far.picked.iter().filter_map(|&option| usize::try_from(option).ok());
            asked.choice = Choice::from_kept(picked, so_far.other, asked.question.options.len());
            if so_far.typed.is_empty() && asked.other.is_none() {
                continue;
            }
            let input = self.other(q, window, cx);
            let end = so_far.typed.len();
            input.update(cx, |input, cx| {
                input.set_value(so_far.typed, window, cx);
                input.set_selected_range(end..end, cx);
            });
        }
        cx.notify();
    }

    /// Asks for what's chosen and typed to be kept, as [`TranscriptEvent::Answering`] says.
    fn answering(&self, at_once: bool, cx: &mut Context<Self>) {
        let _ = self.transcript.update(cx, |_, cx| cx.emit(TranscriptEvent::Answering { at_once }));
    }

    /// Each question's answer, if it has one yet.
    fn answers_so_far(&self, cx: &App) -> Vec<Option<String>> {
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

    /// Sends the answers there are. A question left without one is left out, so the agent can tell.
    fn send(&mut self, cx: &mut Context<Self>) {
        let answered = self.answers_so_far(cx);
        if self.answered || answered.iter().all(Option::is_none) {
            return;
        }
        let asked = self.asked.iter().map(|asked| &asked.question);
        self.answer(ApprovalDecision::Answer { answers: answers(asked.zip(answered)) }, cx);
    }

    fn skip(&mut self, cx: &mut Context<Self>) {
        if !self.answered {
            self.answer(ApprovalDecision::Deny { message: SKIPPED.to_string(), interrupt: false }, cx);
        }
    }

    fn answer(&mut self, decision: ApprovalDecision, cx: &mut Context<Self>) {
        self.answered = true;
        let approval_id = self.approval_id.clone();
        let _ = self.transcript.update(cx, |t, cx| t.answer_approval(approval_id, decision, cx));
        // The hub needn't keep what's been sent.
        self.answering(true, cx);
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
            key => match key.parse::<usize>() {
                Ok(number @ 1..=9) => self.pick(number - 1, window, cx),
                _ => return,
            },
        }
        cx.stop_propagation();
    }

    fn type_other(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.other(self.showing, window, cx);
        input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// The question's row for typing another answer. What's typed there is chosen while there's any, and Enter goes on.
    fn other(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextareaState> {
        if let Some(other) = self.asked.get(q).and_then(|asked| asked.other.as_ref()) {
            return other.input.clone();
        }
        let input =
            cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).submit_on_enter(true).placeholder("Other…"));
        let events = cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| match event {
            // Going back to what's typed chooses it again.
            InputEvent::Change | InputEvent::Focus => {
                let has_text = !input.read(cx).text().to_string().trim().is_empty();
                if let Some(asked) = this.asked.get_mut(q) {
                    asked.choice.typed(has_text, asked.question.multiple);
                }
                this.answering(false, cx);
                cx.notify();
            }
            InputEvent::PressEnter { shift: false, .. } => this.go_on(window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {}
        });
        if let Some(asked) = self.asked.get_mut(q) {
            asked.other = Some(Other { input: input.clone(), _events: events });
        }
        input
    }
}

impl Focusable for QuestionPanel {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl QuestionPanel {
    /// The question's header, a small detail, with the button that skips it across from it, then its text, at body
    /// size.
    fn heading(question: Option<&Question>, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let header = question.and_then(|question| question.header.clone()).unwrap_or_default();
        // A small control's square around an icon as big as the text's line, standing out past the line rather than
        // pushing the question down, so the icon lines up with the edges inside the panel.
        let skip = Button::new("question-skip")
            .hand()
            .ghost()
            .small()
            .size_6()
            .p_0()
            .my(px(-4.))
            .mr(px(-4.))
            .child(Icon::new(IconName::Close).size_4().text_color(muted))
            .tooltip("Skip")
            .on_click(cx.listener(|this, _, _, cx| this.skip(cx)));
        let text = question
            .map(|question| div().w_full().text_sm().line_height(px(QUESTION_LINE)).child(question.text.clone()));
        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .h(px(HEADER_LINE))
                    .gap_2()
                    .child(div().flex_1().min_w_0().truncate().text_xs().text_color(muted).child(header))
                    .child(skip),
            )
            .children(text)
            .into_any_element()
    }
}

/// The height of the header's line, at the kit's extra-small size.
const HEADER_LINE: f32 = 16.;

/// The height of a line of the question, at body size.
const QUESTION_LINE: f32 = 20.;

impl Render for QuestionPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edge = cx.theme().border;
        let answered = self.answers_so_far(cx);
        let ready = !self.answered && answered.iter().any(Option::is_some);
        let q = self.showing;
        let question = self.asked.get(q).map(|asked| asked.question.clone());
        let heading = Self::heading(question.as_ref(), cx);
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
            .children(options)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().children(pager::pager(self, &answered, cx)))
                    .child(send),
            )
    }
}
