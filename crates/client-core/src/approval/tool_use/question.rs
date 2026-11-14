use super::super::Approval;
use crate::calls::{EditFile, Recorded, ToolCall, WriteFile};
use protocol::event::{ApprovalDecision, Asking, EventBody, QuestionAnswer};
use protocol::model::ToolUseId;

/// A tool's use the agent asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolUseAsked<'a> {
    /// The tool, by a name to show: the agent's own for one of its tools, and a readable one for any other.
    pub tool: String,
    /// What the request says the use is for, which for a command is the agent's own line about it. `None` where it
    /// says nothing, which leaves what the tool was given to say it.
    pub about: Option<&'a str>,
    pub given: Option<Given<'a>>,
    /// Why the agent asks, in its own words, when it says.
    pub reason: Option<&'a str>,
    /// The subagent that asks, as the call that started it, or `None` for the agent itself.
    pub asked_by: Option<&'a ToolUseId>,
}

/// What a tool was given, which is what allowing its use allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Given<'a> {
    /// A command, whole, with every line of it.
    Command(&'a str),
    /// An edit to a file, with what each part of it replaces and with what.
    Edit(EditFile<'a>),
    /// A file written whole, with all it will hold.
    Write(WriteFile<'a>),
    /// The input that matters most to any other tool, on one line, such as a URL.
    MainInput(String),
}

impl Given<'_> {
    /// The file it changes, for an edit or a write.
    pub fn path(&self) -> Option<&str> {
        match self {
            Given::Edit(EditFile { path, .. }) | Given::Write(WriteFile { path, .. }) => Some(path),
            Given::Command(_) | Given::MainInput(_) => None,
        }
    }
}

/// The answers to a tool's use, in the order every client shows them, which is how an answer names one, and how the
/// hub keeps the one that's chosen and not yet sent: by its place here. A hub keeps those places, so they never change,
/// and an answer added later goes at the end, as `AnswerSoFar::picked` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolUseAnswer {
    Allow,
    Deny,
}

impl ToolUseAnswer {
    pub const ALL: [ToolUseAnswer; 2] = [ToolUseAnswer::Allow, ToolUseAnswer::Deny];

    /// The answer at `place` in [`ToolUseAnswer::ALL`], or `None` past its end.
    pub fn at(place: u32) -> Option<Self> {
        Self::ALL.get(usize::try_from(place).ok()?).copied()
    }
}

impl<'a> ToolUseAsked<'a> {
    /// What the request `body` asks, where it asks to use a tool, or asks something this build doesn't know, which is
    /// asked about the same way, by its tool and input. `None` for questions, a plan, and any other event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { tool_name, description, reason, asked_by, request, .. } = body else {
            return None;
        };
        let approval = Approval::of(body)?;
        match approval.asking() {
            Asking::ToolUse { .. } | Asking::Unknown => {}
            Asking::Questions { .. } | Asking::Plan { .. } => return None,
        }
        let typed_name = Recorded::asked(body).and_then(|call| match call {
            Recorded::Typed(call) => Some(crate::calls::typed::title(call)),
            Recorded::Legacy { .. } => None,
        });
        let tool_name = typed_name.as_ref().unwrap_or(tool_name);
        let said = description.as_deref().map(str::trim).filter(|said| !said.is_empty());
        let call = ToolCall::read(Recorded::asked(body)?);
        let summary = call.summary();
        let (tool, about, given) = match call {
            ToolCall::Command(command) => {
                (tool_name.clone(), said.or(command.described()), Some(Given::Command(command.command)))
            }
            ToolCall::Edit(edit) => (tool_name.clone(), said, Some(Given::Edit(edit))),
            ToolCall::Write(write) => (tool_name.clone(), said, Some(Given::Write(write))),
            ToolCall::Typed(call) => {
                (crate::calls::typed::title(call), said, Some(Given::MainInput(crate::calls::typed::input(call))))
            }
            ToolCall::Other(other) => (other.name().to_string(), said, summary.map(Given::MainInput)),
            ToolCall::Read(_)
            | ToolCall::Search(_)
            | ToolCall::Agent(_)
            | ToolCall::Fetch(_)
            | ToolCall::WebSearch(_)
            | ToolCall::Advisor => (tool_name.clone(), said, summary.map(Given::MainInput)),
        };
        // A tool with no usual input says what the agent said it's for as its main input, which is no more to show.
        let given = given.filter(|given| !matches!(given, Given::MainInput(input) if Some(input.as_str()) == about));
        Some(Self {
            tool,
            about,
            given,
            reason: request.as_ref().and_then(|r| r.why.as_deref()).or(reason.as_deref()),
            asked_by: asked_by.as_ref(),
        })
    }
}

impl Approval<'_> {
    /// The decision `answer` makes of a tool's use: what's typed denies it in those words, whatever's chosen with it,
    /// and otherwise the one answer chosen says, by its place in [`ToolUseAnswer::ALL`]. `None` for an answer that
    /// chooses none of them, or more than one.
    pub fn deciding_tool_use(&self, answer: &QuestionAnswer) -> Option<ApprovalDecision> {
        if let Some(instead) = &answer.typed {
            return Some(self.refused_saying(instead.clone()));
        }
        let [chosen] = answer.picked[..] else { return None };
        Some(match ToolUseAnswer::at(chosen)? {
            ToolUseAnswer::Allow => self.allowed(),
            ToolUseAnswer::Deny => self.refused(),
        })
    }
}
