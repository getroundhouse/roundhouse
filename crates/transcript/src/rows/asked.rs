//! A question the agent asked, once it's no longer waiting, drawn as a call's line: `Asked`, each question's header in
//! an outlined chip, and what was answered, faint. Opened, a block with each question under its header, and what was
//! answered under it. While it waits it's asked in the composer's place instead, by `questions::QuestionPanel`.

use gpui_kit::assets::IconName;

use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString, Styled,
    TestSupportExt as _, div, px,
};
use protocol::event::Question;
use protocol::model::Seq;

use super::around::{AnswerTo, Asked, Settled};
use super::line::{self, Line, Part};
use super::text::place;
use super::tools::{Toggle, block};
use super::{InRow, Shared, row};
use crate::calls::first_line;
use crate::find::{self, FindLook, Part as Text};
use crate::folds::Fold;
use crate::selection::Pieces;
use crate::texts::Plain;

const ICON: IconName = IconName::MessageCircleQuestionMark;
/// What's said for a question nobody answered.
const NOT_ANSWERED: &str = "Not answered";
/// Between the answers to several questions, on the line.
const BETWEEN: &str = " · ";
/// Puts the arrow beside the first line of an answer at body size.
const ARROW_DROP: f32 = 3.0;

pub(super) fn draw(seq: Seq, asked: &Asked, shared: &Shared, cx: &App) -> AnyElement {
    let questions = &asked.questions;
    // Nothing's found in a question, so nothing it draws is marked.
    let find = FindLook::default();
    let (key, open) = (&asked.key, asked.open);
    // It's in the approval's own row, where each question's texts are placed in a block of their own, by where the
    // question is among them.
    let toggle = Toggle { seq, key, fold: Fold::Output, open, shared, find: &find, in_row: InRow::Own(0) };
    let title = if questions.len() > 1 { format!("Asked {} questions", questions.len()) } else { "Asked".to_string() };
    let mut line = Line::new(ICON, title);
    for header in questions.iter().filter_map(|question| question.header.clone()) {
        line = line.part(Part::Chip(header.into()));
    }
    // Opened, the block says what was answered, so the line doesn't say it twice.
    let said = summary(questions, &asked.settled).filter(|_| !asked.open);
    let line = line.part(said.map(|said| Part::Summary(said.into())));
    let opened = || (!questions.is_empty()).then(|| answers(questions, asked, &toggle, cx));
    // On its own row, spaced as a row of calls is, so it reads as one of them.
    row().pb_2().child(line::drawn(line, None, opened, &toggle, cx)).into_any_element()
}

/// What the line says was answered, in the questions' order, or `None` where that isn't known. A typed answer of
/// several lines shows its first, since that's all a call's line has room for.
fn summary(questions: &[Question], settled: &Settled) -> Option<String> {
    if *settled == Settled::Unknown {
        return None;
    }
    let said = |(at, question): (usize, &Question)| match settled.answer_to(at, question) {
        AnswerTo::Said(answer) => Some(first_line(&answer).to_string()),
        AnswerTo::NotAnswered | AnswerTo::Unknown => None,
    };
    let answered: Vec<String> = questions.iter().enumerate().filter_map(said).collect();
    Some(if answered.is_empty() { NOT_ANSWERED.to_string() } else { answered.join(BETWEEN) })
}

/// Each question under its header, and what was answered under it, after an arrow. One nobody answered says so, and
/// nothing is said where what was answered isn't known.
fn answers(questions: &[Question], asked: &Asked, toggle: &Toggle, cx: &App) -> AnyElement {
    let (key, colors) = (toggle.key, look::colors(cx));
    let id = key.element("asked");
    let area = block::scroll_id(&id);
    let mut list = v_flex().id(id.clone());
    for (i, question) in questions.iter().enumerate() {
        if i > 0 {
            list = list.child(block::separator(cx));
        }
        let answer = asked.settled.answer_to(i, question);
        let toggle = &Toggle { in_row: InRow::Own(i), ..*toggle };
        list = list.child(
            v_flex()
                .px_3()
                .pt_2()
                .pb_3()
                .gap_1p5()
                .when_some(question.header.clone(), |d, header| {
                    d.child(div().text_xs().text_color(colors.text_faint).child(header))
                })
                .child(text(
                    toggle,
                    Text::Question,
                    key.element(&format!("question-{i}")),
                    &question.text,
                    colors.text_muted,
                    &area,
                ))
                .children(answered(toggle, &area, i, answer, cx)),
        );
    }
    let scrolling = block::scrolling(toggle.find, &toggle.shared.selection, &id, list.test_support());
    block::frame(cx).child(scrolling).into_any_element()
}

/// What was answered to the question at `i`, after an arrow, or that nobody answered it, or nothing if that isn't
/// known.
fn answered(toggle: &Toggle, area: &ElementId, i: usize, answer: AnswerTo, cx: &App) -> Option<AnyElement> {
    let (key, colors) = (toggle.key, look::colors(cx));
    let answer = match answer {
        AnswerTo::Said(answer) => answer,
        AnswerTo::Unknown => return None,
        AnswerTo::NotAnswered => {
            let id = key.element(&format!("not-answered-{i}"));
            let said = div().id(id).text_sm().text_color(colors.text_faint).child(NOT_ANSWERED);
            return Some(said.test_support().into_any_element());
        }
    };
    let arrow =
        Icon::new(IconName::CornerDownRight).flex_none().mt(px(ARROW_DROP)).size(px(14.)).text_color(colors.text_faint);
    let row = h_flex()
        .id(key.element(&format!("answer-{i}")))
        .items_start()
        .gap_2()
        .child(arrow)
        .child(
            text(toggle, Text::Answered, key.element(&format!("answer-{i}-text")), &answer, colors.text, area).flex_1(),
        )
        .test_support()
        .into_any_element();
    Some(row)
}

/// Text at body size that can be selected and copied, as a call's output can, the question's `part`, known as `id`, in
/// the area `area`, which scrolls it.
fn text(toggle: &Toggle, part: Text, id: ElementId, text: &str, color: Hsla, area: &ElementId) -> Div {
    let said: SharedString = text.to_string().into();
    let (drawn, layout) = find::marked_with_layout(said.clone(), &toggle.marks(part, &said), &[], Some(area.clone()));
    let (at, area) = (place(toggle, part), area.clone());
    let plain = Plain { at, element: id, area, drawn, said, pieces: Pieces::whole(layout) };
    div().min_w_0().text_sm().text_color(color).child(toggle.shared.texts.draw_plain(&toggle.shared.selection, plain))
}

#[cfg(test)]
mod tests {
    use client_core::Answered;
    use protocol::event::{Question, QuestionAnswer, QuestionOption};

    use super::{Settled, summary};

    fn two_questions() -> Vec<Question> {
        let option = |label: &str| QuestionOption { label: label.into(), description: None };
        let colors = vec![option("Red"), option("Blue")];
        let days = vec![option("Monday"), option("Wednesday"), option("Friday")];
        vec![
            Question { text: "Red or blue?".into(), header: Some("Color".into()), options: colors, multiple: false },
            Question { text: "Which days?".into(), header: Some("Days".into()), options: days, multiple: true },
        ]
    }

    fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
        QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
    }

    fn by_position(answers: Vec<QuestionAnswer>) -> Settled {
        Settled::Answered(Answered::ByPosition(answers))
    }

    #[test]
    fn the_line_says_what_was_answered_in_the_questions_order() {
        let answered = by_position(vec![answer(&[1], None), answer(&[0, 2], None)]);
        let by_text = Settled::Answered(Answered::ByText(
            [("Which days?".into(), "Monday, Friday".into()), ("Red or blue?".into(), "Blue".into())].into(),
        ));

        assert_eq!(summary(&two_questions(), &answered).as_deref(), Some("Blue · Monday, Friday"));
        assert_eq!(summary(&two_questions(), &by_text).as_deref(), Some("Blue · Monday, Friday"), "as a client before");
    }

    #[test]
    fn an_answer_of_several_lines_shows_its_first_on_the_line() {
        let typed = by_position(vec![answer(&[], Some("Neither\nGreen, if it's there"))]);

        assert_eq!(summary(&two_questions(), &typed).as_deref(), Some("Neither"));
    }

    #[test]
    fn the_line_leaves_out_what_was_left_out_and_says_when_nothing_was_answered() {
        let one = by_position(vec![QuestionAnswer::default(), answer(&[0], None)]);

        assert_eq!(summary(&two_questions(), &one).as_deref(), Some("Monday"));
        assert_eq!(summary(&two_questions(), &by_position(Vec::new())).as_deref(), Some("Not answered"));
        assert_eq!(summary(&two_questions(), &Settled::NotAnswered).as_deref(), Some("Not answered"));
        assert_eq!(summary(&two_questions(), &Settled::Unknown), None, "nothing, where it isn't known");
    }
}
