use super::super::heading::said;
use super::super::selectable::SelectableText;
use crate::rows::ChangeAsked;
use crate::scroll_area::holding_wide;
use client_core::{Approval, Given, ToolUseAnswer, ToolUseAsked};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Role, StatefulInteractiveElement as _,
    Styled, TestSupportExt as _, div, px,
};
use look::{Shaped as _, Typeset as _};
use protocol::event::{ApprovalDecision, EventBody, Question, QuestionAnswer, QuestionOption};

/// What the typing row says while it's empty.
pub(in super::super) const INSTEAD: &str = "Deny, and say what to do instead…";

/// What's asked of a use the agent said nothing about, and that has no input of its own to ask it by.
const RUN_THIS: &str = "Run this command?";
const MAKE_THIS: &str = "Make this edit?";
const WRITE_THIS: &str = "Write this file?";
const USE_THIS: &str = "Use this tool?";

/// About seven lines of a command, or of an edit's diff. A longer one scrolls, so the answers stay in sight.
const GIVEN_MOST_HEIGHT: f32 = 160.;

/// A tool's use the agent asks about.
pub(crate) struct ToolUse {
    /// What it asks, which makes the decision that answers it.
    pub(super) approval: Approval<'static>,
    pub(in super::super) can_type: bool,
    /// The question it's asked as: the tool as its header, what the use is for as its text, and a row for each of
    /// [`ToolUseAnswer::ALL`] as its options.
    pub(super) question: Question,
    /// What the tool was given, when the question's text isn't already that.
    pub(super) given: Option<Shown>,
    /// Why the agent asks, in its own words.
    pub(super) reason: Option<String>,
}

/// What a tool was given, as it's shown.
pub(super) enum Shown {
    /// A command, or another tool's main input, as it was given.
    Text(String),
    /// What an edit or a write would change, under its file.
    Change(ChangeAsked),
}

impl ToolUse {
    /// What the request `body` asks, where it asks to use a tool, as `approval` says, or `None` for any other.
    pub(in super::super) fn read(body: &EventBody, approval: Approval<'static>, can_type: bool) -> Option<Self> {
        let asked = ToolUseAsked::of(body)?;
        let runs_a_command = matches!(asked.given, Some(Given::Command(_)));
        let (text, given) = match (asked.about, asked.given) {
            (about, Some(Given::Command(command))) => {
                (about.unwrap_or(RUN_THIS).to_string(), Some(Shown::Text(command.into())))
            }
            (about, Some(given @ (Given::Edit(_) | Given::Write(_)))) => {
                let asks = if matches!(given, Given::Edit(_)) { MAKE_THIS } else { WRITE_THIS };
                (about.unwrap_or(asks).to_string(), ChangeAsked::of(&given).map(Shown::Change))
            }
            (Some(about), Some(Given::MainInput(input))) => (about.to_string(), Some(Shown::Text(input))),
            // With nothing said of what it's for, its main input is what's asked about.
            (None, Some(Given::MainInput(input))) => (input, None),
            (about, None) => (about.unwrap_or(USE_THIS).to_string(), None),
        };
        let option = |answer: &ToolUseAnswer| {
            let (label, means) = match answer {
                ToolUseAnswer::Allow if runs_a_command => ("Allow", "Runs it this once"),
                ToolUseAnswer::Allow => ("Allow", "This once"),
                ToolUseAnswer::Deny => ("Deny", "The agent is told it can't, and carries on"),
            };
            QuestionOption { label: label.into(), description: Some(means.into()) }
        };
        let options = ToolUseAnswer::ALL.iter().map(option).collect();
        let question = Question { text, header: Some(asked.tool), options, multiple: false };
        Some(Self { approval, can_type, question, given, reason: asked.reason.map(str::to_string) })
    }

    pub(in super::super) fn question(&self) -> &Question {
        &self.question
    }

    /// The decision `answer` makes, or `None` for one that chooses neither answer, as `client_core` says.
    pub(in super::super) fn decision(&self, answer: &QuestionAnswer) -> Option<ApprovalDecision> {
        self.approval.deciding_tool_use(answer)
    }

    /// What the tool was given, in the code font, in an area that scrolls a long one, then why the agent asks, as a
    /// small detail. `None` with neither to show.
    pub(in super::super) fn detail(&self, cx: &App) -> Option<AnyElement> {
        if self.given.is_none() && self.reason.is_none() {
            return None;
        }
        let colors = look::colors(cx);
        let block = |id: &'static str| {
            div()
                .id(id)
                .w_full()
                .max_h(px(GIVEN_MOST_HEIGHT))
                .py_2()
                .corners(look::Corner::Control)
                .bg(colors.inset)
                .typeset(look::Type::Code)
        };
        let given = self.given.as_ref().map(|given| match given {
            Shown::Text(text) => block("question-given")
                .role(Role::Label)
                .aria_label(text.clone())
                .overflow_y_scroll()
                .px_3()
                .child(SelectableText::new("question-given-text", text.clone()))
                .test_support()
                .into_any_element(),
            // Its file, then its diff, which scrolls both ways, as an edit's does in the transcript.
            Shown::Change(change) => v_flex()
                .w_full()
                .gap_1()
                .child(
                    said("question-change-file", change.path.clone())
                        .typeset(look::Type::Detail)
                        .text_color(colors.text_muted)
                        .test_support(),
                )
                .child(
                    holding_wide(
                        block("question-change").overflow_scroll().restrict_scroll_to_axis(),
                        div().children(change.rows(cx)),
                    )
                    .test_support(),
                )
                .into_any_element(),
        });
        let reason = self.reason.clone().map(|reason| {
            said("question-reason", reason).typeset(look::Type::Detail).text_color(colors.text_muted).test_support()
        });
        Some(v_flex().w_full().gap_3().children(given).children(reason).into_any_element())
    }
}
