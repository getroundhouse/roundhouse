//! How a tool's result says it failed, for the line under its call: how a command exited, when it was one, and what
//! went wrong. The agent's adapter says both. A result recorded before adapters said has them in its text, as Claude
//! wrote it then, which [`super::older`] reads.

use super::older;

/// A failed result, as it's recorded.
#[derive(Clone, Copy)]
pub struct Failed<'a> {
    /// What came back.
    pub text: &'a str,
    /// How the command exited, as the result says.
    pub exit_code: Option<i32>,
    /// Whether the result says what kind of call it answers, which is how one from an adapter that says is told from
    /// one recorded before, whose text holds how it failed.
    pub kind_said: bool,
}

pub struct Failure<'a> {
    /// The command's exit code, when it was a command that exited.
    exit_code: Option<i32>,
    /// What went wrong, on one line.
    pub message: &'a str,
}

impl<'a> Failure<'a> {
    pub fn read(failed: Failed<'a>) -> Self {
        let exit_code = if failed.kind_said { failed.exit_code } else { older::exit_code(failed.text) };
        Self { exit_code, message: super::first_line(output(failed)) }
    }

    /// What its line says first, such as `Exit 2`.
    pub fn label(&self) -> String {
        self.exit_code.map_or_else(|| "Error".to_string(), |code| format!("Exit {code}"))
    }
}

/// What went wrong, in full, as an opened call shows it: without how a command exited, which the line under the call
/// already says, and without anything the agent put around a tool's own refusal.
pub fn output(failed: Failed<'_>) -> &str {
    if failed.kind_said { failed.text } else { older::output(failed.text) }
}

#[cfg(test)]
mod tests {
    use super::{Failed, Failure, output};

    #[test]
    fn a_failed_command_says_how_it_exited_and_what_it_printed() {
        let text = "ls: cannot access 'missing.rs': No such file or directory\nmore";
        let failed = Failed { text, exit_code: Some(2), kind_said: true };

        let failure = Failure::read(failed);

        assert_eq!(failure.label(), "Exit 2");
        assert_eq!(failure.message, "ls: cannot access 'missing.rs': No such file or directory");
        assert_eq!(output(failed), text);
    }

    #[test]
    fn a_tool_that_refused_says_why() {
        let failed = Failed { text: "regex parse error:\n    Stop(", exit_code: None, kind_said: true };

        let failure = Failure::read(failed);

        assert_eq!(failure.label(), "Error");
        assert_eq!(failure.message, "regex parse error:");
    }

    /// What the adapter gave is what came back alone, so nothing in it is taken for how it failed.
    #[test]
    fn what_came_back_from_an_adapter_that_says_is_shown_as_it_is() {
        let text = "Exit code 7\n<tool_use_error>printed by the command itself</tool_use_error>";
        let failed = Failed { text, exit_code: Some(1), kind_said: true };

        assert_eq!(Failure::read(failed).label(), "Exit 1");
        assert_eq!(output(failed), text);
    }

    #[test]
    fn a_result_from_before_adapters_said_is_read_from_its_text() {
        let command = Failed { text: "Exit code 2\nls: no such file", exit_code: None, kind_said: false };
        assert_eq!(Failure::read(command).label(), "Exit 2");
        assert_eq!(Failure::read(command).message, "ls: no such file");
        assert_eq!(output(command), "ls: no such file");

        let refused =
            Failed { text: "<tool_use_error>no such tool</tool_use_error>", exit_code: None, kind_said: false };
        assert_eq!(Failure::read(refused).label(), "Error");
        assert_eq!(output(refused), "no such tool");
    }
}
