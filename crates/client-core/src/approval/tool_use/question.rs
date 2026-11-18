use super::super::Approval;
use crate::calls::{EditFile, Recorded, ToolCall, WriteFile};
use protocol::capability::approvals::Decision;
use protocol::capability::asking::QuestionAnswer;
use protocol::capability::calls::call::Patch;
use protocol::event::EventBody;
use protocol::model::{SubagentId, UseChoice};

/// A tool's use the agent asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolUseAsked<'a> {
    /// The tool, by a name to show: the agent's own for one of its tools, and a readable one for any other.
    pub tool: String,
    /// What the use is for, which for a command is the agent's own line about it. `None` where it says nothing, which
    /// leaves what the tool was given to say it.
    pub about: Option<&'a str>,
    pub given: Option<Given<'a>>,
    /// Why the agent asks, in its own words, when it says.
    pub reason: Option<&'a str>,
    /// The subagent that asks, or `None` for the agent itself.
    pub asked_by: Option<&'a SubagentId>,
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
    /// Files changed at once, each added, deleted, or changed and maybe moved, with each change.
    Patch(&'a Patch),
    /// The input that matters most to any other tool, on one line, such as a URL.
    MainInput(String),
}

impl Given<'_> {
    /// The file it changes, for an edit, a write, or a patch of one file.
    pub fn path(&self) -> Option<&str> {
        match self {
            Given::Edit(EditFile { path, .. }) | Given::Write(WriteFile { path, .. }) => Some(path),
            Given::Patch(Patch { files }) => match files.as_slice() {
                [file] => Some(&file.path),
                _ => None,
            },
            Given::Command(_) | Given::MainInput(_) => None,
        }
    }
}

/// The answers to a tool's use, in the order every client shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolUseAnswer {
    Allow,
    Deny,
}

impl ToolUseAnswer {
    pub const ALL: [ToolUseAnswer; 2] = [ToolUseAnswer::Allow, ToolUseAnswer::Deny];

    /// The answer at `place` in [`ToolUseAnswer::ALL`], or `None` past its end.
    pub fn at(place: usize) -> Option<Self> {
        Self::ALL.get(place).copied()
    }

    /// Its place in [`ToolUseAnswer::ALL`].
    pub fn place(self) -> usize {
        match self {
            ToolUseAnswer::Allow => 0,
            ToolUseAnswer::Deny => 1,
        }
    }

    /// It, as a draft keeps what's chosen.
    pub fn chosen(self) -> UseChoice {
        match self {
            ToolUseAnswer::Allow => UseChoice::Allow,
            ToolUseAnswer::Deny => UseChoice::Deny,
        }
    }

    /// What a draft keeps as chosen, or `None` for a choice this build doesn't know.
    pub fn of(chosen: UseChoice) -> Option<Self> {
        match chosen {
            UseChoice::Allow => Some(ToolUseAnswer::Allow),
            UseChoice::Deny => Some(ToolUseAnswer::Deny),
            UseChoice::Unknown => None,
        }
    }
}

impl<'a> ToolUseAsked<'a> {
    /// What the request `body` asks, where it asks to make a call. `None` for anything else it asks, and for any other
    /// event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { asked_by, request, .. } = body else { return None };
        let Recorded::Typed(typed) = Recorded::asked(body)?;
        let call = ToolCall::read(Recorded::Typed(typed));
        let summary = call.summary();
        let tool = crate::calls::typed::title(typed);
        let (tool, about, given) = match call {
            ToolCall::Command(command) => (tool, command.described(), Some(Given::Command(command.command))),
            ToolCall::Edit(edit) => (tool, None, Some(Given::Edit(edit))),
            ToolCall::Write(write) => (tool, None, Some(Given::Write(write))),
            ToolCall::Patch(patch) => (tool, None, Some(Given::Patch(patch))),
            // Asked about by its typed input, which holds what it puts in the cell.
            ToolCall::NotebookEdit(_) => (tool, None, Some(Given::MainInput(crate::calls::typed::input(typed)))),
            ToolCall::Other(other) => (other.name().to_string(), None, summary.map(Given::MainInput)),
            ToolCall::Read(_)
            | ToolCall::Search(_)
            | ToolCall::Agent(_)
            | ToolCall::ToSubagents(_)
            | ToolCall::MakeImage(_)
            | ToolCall::Typed(_)
            | ToolCall::Fetch(_)
            | ToolCall::WebSearch(_)
            | ToolCall::Advisor => (tool, None, summary.map(Given::MainInput)),
        };
        let about = about.map(str::trim).filter(|about| !about.is_empty());
        Some(Self { tool, about, given, reason: request.why.as_deref(), asked_by: asked_by.as_ref() })
    }
}

impl Approval<'_> {
    /// The decision `answer` makes of a tool's use: what's typed denies it in those words, whatever's chosen with it,
    /// and otherwise the one answer chosen says, by its place in [`ToolUseAnswer::ALL`]. `None` for an answer that
    /// chooses none of them, or more than one.
    pub fn deciding_tool_use(&self, answer: &QuestionAnswer) -> Option<Decision> {
        if let Some(instead) = &answer.typed {
            return Some(self.refuse(Some(instead.clone())));
        }
        let [chosen] = answer.picked[..] else { return None };
        match ToolUseAnswer::at(usize::try_from(chosen).ok()?)? {
            ToolUseAnswer::Allow => self.approve(),
            ToolUseAnswer::Deny => Some(self.refuse(None)),
        }
    }
}
