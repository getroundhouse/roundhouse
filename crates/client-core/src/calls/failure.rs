//! How a tool's result says it failed, for the line under its call: how a command exited, when it was one, and what
//! went wrong. The agent's adapter says both, and a result that says what kind of call it answers holds what came
//! back alone. One with no kind was recorded before adapters said, and has both in its text, as Claude wrote it then,
//! which [`super::older`] reads.

use protocol::event::ContentBlock;

use super::older;

/// What a failed result says went wrong.
#[derive(Clone, Copy, Debug)]
pub struct Failure<'a> {
    /// The command's exit code, when it was a command that exited.
    pub exit_code: Option<i32>,
    /// What went wrong, on one line.
    pub message: &'a str,
    /// What went wrong, in full, as an opened call shows it: without how a command exited, which the line under the
    /// call already says, and without anything the agent put around a tool's own refusal.
    pub output: &'a str,
}

impl<'a> Failure<'a> {
    /// What the result `block` says went wrong, or `None` if it isn't a result, or didn't fail. A call the person
    /// denied didn't fail: it never ran, and its result says what they said rather than what went wrong.
    pub fn of(block: &'a ContentBlock) -> Option<Self> {
        let ContentBlock::ToolResult { is_error: true, text, denied: None, .. } = block else { return None };
        Some(Self::read(text, ResultForm::of(block)?))
    }

    /// The same, from the text of a failed result and the form it was recorded in, for a view that keeps a copy of
    /// both rather than the block.
    pub fn read(text: &'a str, form: ResultForm) -> Self {
        let (exit_code, output) = match form.0 {
            Form::Alone { exit_code } => (exit_code, text),
            Form::AsSent => (older::exit_code(text), older::output(text)),
        };
        Self { exit_code, message: super::first_line(output), output }
    }

    /// What its line says first, such as `Exit 2`.
    pub fn label(&self) -> String {
        self.exit_code.map_or_else(|| "Error".to_string(), |code| format!("Exit {code}"))
    }
}

/// The form a result was recorded in, which says how its text is read. Only [`ResultForm::of`] makes one, from the
/// result itself, so no client decides it.
#[derive(Clone, Copy, Debug)]
pub struct ResultForm(Form);

#[derive(Clone, Copy, Debug)]
enum Form {
    /// Through an adapter that says: its text is what came back alone, and how a command exited is beside it.
    Alone { exit_code: Option<i32> },
    /// Before adapters said, as Claude sent it, with how it failed in its text.
    AsSent,
}

impl ResultForm {
    /// The form of the result `block`, or `None` for any other block.
    pub fn of(block: &ContentBlock) -> Option<Self> {
        if let ContentBlock::CallResult { output, .. } = block {
            let exit_code = match output {
                protocol::capability::calls::CallOutput::Command(c) => c.exit_code,
                protocol::capability::calls::CallOutput::Lines(_)
                | protocol::capability::calls::CallOutput::Image { .. }
                | protocol::capability::calls::CallOutput::Document { .. }
                | protocol::capability::calls::CallOutput::Changed { .. }
                | protocol::capability::calls::CallOutput::Found(_)
                | protocol::capability::calls::CallOutput::Paths { .. }
                | protocol::capability::calls::CallOutput::Page { .. }
                | protocol::capability::calls::CallOutput::WebResults { .. }
                | protocol::capability::calls::CallOutput::Report { .. }
                | protocol::capability::calls::CallOutput::Mcp { .. }
                | protocol::capability::calls::CallOutput::Words { .. }
                | protocol::capability::calls::CallOutput::Unknown => None,
            };
            return Some(Self(Form::Alone { exit_code }));
        }
        let ContentBlock::ToolResult { call, exit_code, .. } = block else { return None };
        Some(Self(if call.is_some() { Form::Alone { exit_code: *exit_code } } else { Form::AsSent }))
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::CallKind;
    use protocol::fixtures;

    use super::{Failure, Form, ResultForm};

    const AS_SENT: ResultForm = ResultForm(Form::AsSent);

    fn alone(exit_code: Option<i32>) -> ResultForm {
        ResultForm(Form::Alone { exit_code })
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

    /// A result's form is whether it says what kind of call it answers, whatever that kind is.
    #[test]
    fn a_recorded_result_is_read_by_whether_it_says_its_kind() {
        let before = fixtures::tool_result("c", "Exit code 2\nno such file", true);
        let since = fixtures::of_kind(fixtures::command_failed("c", "no such file", 2), CallKind::Unknown);

        let (before, since) = (Failure::of(&before).unwrap(), Failure::of(&since).unwrap());

        assert_eq!((before.exit_code, before.output), (Some(2), "no such file"));
        assert_eq!((since.exit_code, since.output), (Some(2), "no such file"));
        assert!(Failure::of(&fixtures::tool_result("c", "a.rs", false)).is_none(), "it didn't fail");
        assert!(Failure::of(&fixtures::tool_denied("c", "Not that", Some("Not that"))).is_none(), "it was denied");
        assert!(Failure::of(&fixtures::tool_use("c", "Bash", serde_json::json!({}))).is_none(), "not a result");
    }
}
