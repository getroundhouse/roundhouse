//! Questions the agent asks the user. They arrive as a tool waiting for approval, but what they need
//! is an answer: allowing one without an answer tells the agent the user didn't reply.

use std::collections::BTreeMap;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, ElementId, FontWeight, Hsla, IntoElement, ParentElement, Styled, WeakEntity, div, px};
use look::{Hand as _, Off as _};
use protocol::event::ApprovalDecision;
use protocol::model::{ApprovalId, Seq};
use serde_json::Value;

use crate::Transcript;
use crate::rows::{row, waiting_card};
use look::Signal;

pub(crate) const TOOL_NAME: &str = "AskUserQuestion";

/// Told to the agent when the user would rather not pick anything.
const SKIPPED: &str = "The user chose not to answer. Carry on if you can, or ask in chat.";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Question {
    pub text: String,
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// Whatever the agent sent. A question with no options can still be answered by typing.
pub(crate) fn parse(input: &Value) -> Vec<Question> {
    let text_of = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let questions = input.get("questions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    questions
        .iter()
        .filter_map(|question| {
            let options = question.get("options").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            Some(Question {
                text: text_of(question, "question")?,
                header: text_of(question, "header"),
                options: options
                    .iter()
                    .filter_map(|option| {
                        Some(QuestionOption {
                            label: text_of(option, "label")?,
                            description: text_of(option, "description"),
                        })
                    })
                    .collect(),
                multiple: question.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// With one answer allowed, a new choice replaces the old. With several, choosing again takes it back.
pub(crate) fn toggle_pick(picks: &mut Vec<String>, label: String, multiple: bool) {
    if !multiple {
        *picks = vec![label];
    } else if let Some(at) = picks.iter().position(|pick| *pick == label) {
        picks.remove(at);
    } else {
        picks.push(label);
    }
}

pub(crate) fn answers_from(picks: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, String> {
    picks.iter().map(|(question, labels)| (question.clone(), labels.join(", "))).collect()
}

/// Choosing something and then taking it back leaves it unanswered.
pub(crate) fn first_unanswered<'a>(
    asked: &'a [Question],
    picks: &BTreeMap<String, Vec<String>>,
) -> Option<&'a Question> {
    asked.iter().find(|question| picks.get(&question.text).is_none_or(std::vec::Vec::is_empty))
}

/// Which card this is, and what its buttons act on.
struct CardContext<'a> {
    /// The edge of each answer's number.
    border: Hsla,
    seq: Seq,
    approval_id: &'a ApprovalId,
    open: bool,
    only_question: bool,
    this: &'a WeakEntity<Transcript>,
}

fn option_button(
    card: &CardContext<'_>,
    q: usize,
    o: usize,
    question: &Question,
    option: &QuestionOption,
    selected: bool,
    muted: Hsla,
) -> AnyElement {
    let (seq, only_question) = (card.seq, card.only_question);
    let (this, approval_id, question, label) =
        (card.this.clone(), card.approval_id.clone(), question.clone(), option.label.clone());
    let border = card.border;
    Button::new(ElementId::Name(format!("answer-{seq}-{q}-{o}").into()))
        .hand()
        .small()
        // Two lines, which is more than a small button's fixed height.
        .h_auto()
        .w_full()
        .selected(selected)
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |t, cx| {
                t.pick_answer(seq, approval_id.clone(), &question, label.clone(), only_question, cx);
            });
        })
        .child(
            h_flex()
                .w_full()
                .py_1p5()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .mt(px(1.))
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{}", o + 1)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().text_sm().child(option.label.clone()))
                        .when_some(option.description.clone(), |d, description| {
                            d.child(div().text_xs().text_color(muted).child(description))
                        }),
                ),
        )
        .into_any_element()
}

fn question_view(
    card: &CardContext<'_>,
    q: usize,
    question: &Question,
    chosen: Option<&Vec<String>>,
    muted: Hsla,
) -> AnyElement {
    let typed = chosen
        .into_iter()
        .flatten()
        .filter(|pick| !question.options.iter().any(|option| option.label == **pick))
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let options: Vec<AnyElement> = question
        .options
        .iter()
        .enumerate()
        .map(|(o, option)| {
            let selected = chosen.is_some_and(|labels| labels.contains(&option.label));
            option_button(card, q, o, question, option, selected, muted)
        })
        .collect();
    v_flex()
        .w_full()
        .gap_2()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(question.text.clone()))
        .when(card.open && !options.is_empty(), |d| d.child(v_flex().w_full().gap_1().children(options)))
        .when(card.open && !typed.is_empty(), |d| {
            d.child(div().text_xs().text_color(muted).child(format!("Your answer: {typed}")))
        })
        .into_any_element()
}

fn card_actions(card: &CardContext<'_>, needs_send: bool, ready: bool) -> impl IntoElement {
    let seq = card.seq;
    let (to_send, to_skip) = (card.approval_id.clone(), card.approval_id.clone());
    let (send, skip) = (card.this.clone(), card.this.clone());
    h_flex()
        .w_full()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(look::faint())
                .child("Or type your own answer in the box below."),
        )
        .child(
            Button::new(ElementId::Name(format!("skip-question-{seq}").into()))
                .hand()
                .small()
                .ghost()
                .label("Skip")
                .on_click(move |_, _, cx| {
                    let decision = ApprovalDecision::Deny { message: SKIPPED.to_string(), interrupt: false };
                    let _ = skip.update(cx, |t, cx| t.answer_approval(to_skip.clone(), decision, cx));
                }),
        )
        .when(needs_send, |d| {
            d.child(
                Button::new(ElementId::Name(format!("send-answers-{seq}").into()))
                    .small()
                    .primary()
                    .off_while(!ready)
                    .label("Send answers")
                    .on_click(move |_, _, cx| {
                        let _ = send.update(cx, |t, cx| t.send_answers(to_send.clone(), cx));
                    }),
            )
        })
}

pub(crate) fn card(
    seq: Seq,
    approval_id: &ApprovalId,
    input: &Value,
    open: bool,
    picks: &BTreeMap<String, Vec<String>>,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let amber = theme.warning;
    let asked = parse(input);
    let only_question = asked.len() == 1;
    let needs_send = !only_question || asked.iter().any(|q| q.multiple);
    let ready = first_unanswered(&asked, picks).is_none();
    let header = asked.iter().find_map(|q| q.header.clone()).unwrap_or_else(|| "Question".to_string());
    let context = CardContext { border: theme.border, seq, approval_id, open, only_question, this };
    let rendered: Vec<AnyElement> = asked
        .iter()
        .enumerate()
        .map(|(q, question)| question_view(&context, q, question, picks.get(&question.text), muted))
        .collect();
    row()
        .child(
            waiting_card(open, cx)
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if open { amber } else { muted })
                        .child(look::mark(
                            if open { Signal::NeedsYou } else { Signal::Idle },
                            look::Work::NoPr,
                            look::Size::Inline,
                            cx,
                        ))
                        .child(if open { format!("The agent is asking · {header}") } else { header }),
                )
                .children(rendered)
                .when(open, |d| d.child(card_actions(&context, needs_send, ready))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{answers_from, first_unanswered, parse, toggle_pick};

    #[test]
    fn takes_what_the_cli_sends_and_tolerates_what_it_might() {
        let input = json!({"questions":[
            {"question":"Red or blue?","header":"Color","multiSelect":true,
             "options":[{"label":"Red","description":"Warm"},{"label":"Blue"},{"description":"no label"}]},
            {"question":"Anything else?"},
            {"header":"no question text"},
        ]});
        let asked = parse(&input);
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0].text, "Red or blue?");
        assert!(asked[0].multiple);
        assert_eq!(asked[0].options.len(), 2);
        assert_eq!(asked[0].options[0].description.as_deref(), Some("Warm"));
        assert!(asked[1].options.is_empty() && !asked[1].multiple);
        assert!(parse(&json!({"something":"else"})).is_empty());
    }

    #[test]
    fn one_answer_replaces_and_several_toggle() {
        let mut picks = vec!["Red".to_string()];
        toggle_pick(&mut picks, "Blue".to_string(), false);
        assert_eq!(picks, ["Blue"]);
        toggle_pick(&mut picks, "Red".to_string(), true);
        assert_eq!(picks, ["Blue", "Red"]);
        toggle_pick(&mut picks, "Blue".to_string(), true);
        assert_eq!(picks, ["Red"]);
    }

    #[test]
    fn several_choices_are_joined_and_nothing_chosen_isnt_an_answer() {
        let asked = parse(&json!({"questions":[{"question":"Q","multiSelect":true}]}));
        let mut picks = BTreeMap::new();
        assert!(first_unanswered(&asked, &picks).is_some());
        picks.insert("Q".to_string(), Vec::new());
        assert!(first_unanswered(&asked, &picks).is_some(), "chose something, then took it back");
        picks.insert("Q".to_string(), vec!["A".to_string(), "B".to_string()]);
        assert!(first_unanswered(&asked, &picks).is_none());
        assert_eq!(answers_from(&picks)["Q"], "A, B");
    }
}
