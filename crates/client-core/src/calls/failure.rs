//! How a tool's result says it failed, for the line under its call: how a command ended, when it was one, and what
//! went wrong. The agent's adapter says both, and a result that says what kind of call it answers holds what came back
//! alone. One with no kind was recorded before adapters said, and has both in its text, as Claude wrote it then, which
//! [`super::older`] reads.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::Ended;
use protocol::event::ContentBlock;

use super::older;

/// What a failed result says went wrong.
#[derive(Clone, Copy, Debug)]
pub struct Failure<'a> {
    /// How the command ended, when it was one and its agent said.
    pub ended: Option<Ending>,
    /// What went wrong, on one line.
    pub message: &'a str,
    /// What went wrong, in full, as an opened call shows it: without how a command ended, which the line under the
    /// call already says, and without anything the agent put around a tool's own refusal.
    pub output: &'a str,
}

/// How a failed command ended. It's the client's own, rather than the protocol's [`Ended`], so a [`Failure`] stays a
/// copy that borrows only its text, and only what a failure's line says is here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Exited(i32),
    TimedOut,
    /// Someone stopped it, the person or the agent.
    Stopped,
}

impl<'a> Failure<'a> {
    /// What a failed result says went wrong, from its text and the form it was recorded in, as
    /// [`super::Returned::failure`] reads it, for a view that keeps a copy of both rather than the block.
    pub fn read(text: &'a str, form: ResultForm) -> Self {
        let (ended, output) = match form.0 {
            Form::Alone { ended } => (ended, text),
            Form::AsSent => (older::exit_code(text).map(Ending::Exited), older::output(text)),
        };
        Self { ended, message: super::first_line(output), output }
    }

    /// What its line says first, such as `Exit 2`, or `Timed out` for a command that didn't exit.
    pub fn label(&self) -> String {
        match self.ended {
            Some(Ending::Exited(code)) => format!("Exit {code}"),
            Some(Ending::TimedOut) => "Timed out".to_string(),
            Some(Ending::Stopped) => "Stopped".to_string(),
            None => "Error".to_string(),
        }
    }
}

/// The form a result was recorded in, which says how its text is read. Only the result itself says, so no client
/// decides it: a client has one from [`super::Returned::form`].
#[derive(Clone, Copy, Debug)]
pub struct ResultForm(Form);

#[derive(Clone, Copy, Debug)]
enum Form {
    /// Through an adapter that says: its text is what came back alone, and how a command ended is beside it.
    Alone { ended: Option<Ending> },
    /// Before adapters said, as Claude sent it, with how it failed in its text.
    AsSent,
}

impl ResultForm {
    /// The form of the result `block`, or `None` for any other block.
    pub(super) fn of(block: &ContentBlock) -> Option<Self> {
        match block {
            ContentBlock::CallResult { output, .. } => Some(Self(Form::Alone { ended: ending(output) })),
            ContentBlock::ToolResult { call: Some(_), exit_code, .. } => {
                Some(Self(Form::Alone { ended: exit_code.map(Ending::Exited) }))
            }
            ContentBlock::ToolResult { call: None, .. } => Some(Self(Form::AsSent)),
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Call { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => None,
        }
    }
}

/// How a typed command ended, for its failure's line. That it timed out or was stopped says more than any exit code it
/// has.
fn ending(output: &CallOutput) -> Option<Ending> {
    let CallOutput::Command(command) = output else { return None };
    match command.ended {
        Ended::TimedOut => Some(Ending::TimedOut),
        Ended::Stopped => Some(Ending::Stopped),
        Ended::Exited | Ended::Backgrounded { .. } | Ended::Unknown => command.exit_code.map(Ending::Exited),
    }
}

#[cfg(test)]
mod tests {
    use super::{Ending, Failure, Form, ResultForm};

    const AS_SENT: ResultForm = ResultForm(Form::AsSent);

    fn alone(exit_code: Option<i32>) -> ResultForm {
        ResultForm(Form::Alone { ended: exit_code.map(Ending::Exited) })
    }

    #[test]
    fn a_failed_command_says_how_it_exited_and_what_it_printed() {
        let text = "ls: cannot access 'missing.rs': No such file or directory\nmore";

        let failure = Failure::read(text, alone(Some(2)));

        assert_eq!(failure.label(), "Exit 2");
        assert_eq!(failure.message, "ls: cannot access 'missing.rs': No such file or directory");
        assert_eq!(failure.output, text);
    }

    #[test]
    fn a_tool_that_refused_says_why() {
        let failure = Failure::read("regex parse error:\n    Stop(", alone(None));

        assert_eq!(failure.label(), "Error");
        assert_eq!(failure.message, "regex parse error:");
    }

    /// What the adapter gave is what came back alone, so nothing in it is taken for how it failed.
    #[test]
    fn what_came_back_through_an_adapter_that_says_is_shown_as_it_is() {
        let text = "Exit code 7\n<tool_use_error>printed by the command itself</tool_use_error>";

        let failure = Failure::read(text, alone(Some(1)));

        assert_eq!((failure.label().as_str(), failure.output), ("Exit 1", text));
    }

    #[test]
    fn a_result_from_before_adapters_said_is_read_from_its_text() {
        let command = Failure::read("Exit code 2\nls: no such file", AS_SENT);
        assert_eq!(
            (command.label().as_str(), command.message, command.output),
            ("Exit 2", "ls: no such file", "ls: no such file")
        );

        let refused = Failure::read("<tool_use_error>no such tool</tool_use_error>", AS_SENT);
        assert_eq!((refused.label().as_str(), refused.output), ("Error", "no such tool"));
    }
}
