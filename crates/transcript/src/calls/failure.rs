//! How a tool's result says it failed, for the line under its call. Claude starts a failed command's result with
//! `Exit code N`, and wraps a tool's own refusal in `<tool_use_error>`.

const EXIT_CODE: &str = "Exit code ";
const TOOL_ERROR_OPEN: &str = "<tool_use_error>";
const TOOL_ERROR_CLOSE: &str = "</tool_use_error>";

pub(crate) struct Failure<'a> {
    /// The command's exit code, when it was a command that exited.
    exit_code: Option<i32>,
    /// What went wrong, on one line.
    pub(crate) message: &'a str,
}

impl<'a> Failure<'a> {
    pub(crate) fn read(text: &'a str) -> Self {
        let exit_code = exit_code(text);
        let rest = if exit_code.is_some() { after_first_line(text) } else { text };
        let rest = rest.trim().trim_start_matches(TOOL_ERROR_OPEN).trim_end_matches(TOOL_ERROR_CLOSE);
        Self { exit_code, message: super::first_line(rest) }
    }

    /// What its line says first, such as `Exit 2`.
    pub(crate) fn label(&self) -> String {
        self.exit_code.map_or_else(|| "Error".to_string(), |code| format!("Exit {code}"))
    }
}

/// A command's output without the line saying how it exited, which the line under the call already says.
pub(crate) fn output(text: &str) -> &str {
    if exit_code(text).is_some() { after_first_line(text) } else { text }
}

fn exit_code(text: &str) -> Option<i32> {
    text.lines().next()?.trim().strip_prefix(EXIT_CODE)?.parse().ok()
}

fn after_first_line(text: &str) -> &str {
    text.split_once('\n').map_or("", |(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::{Failure, output};

    #[test]
    fn a_failed_command_says_how_it_exited_and_what_it_printed() {
        let text = "Exit code 2\nls: cannot access 'missing.rs': No such file or directory";

        let failure = Failure::read(text);

        assert_eq!(failure.label(), "Exit 2");
        assert_eq!(failure.message, "ls: cannot access 'missing.rs': No such file or directory");
        assert_eq!(output(text), "ls: cannot access 'missing.rs': No such file or directory");
    }

    #[test]
    fn a_tool_that_refused_says_why() {
        let failure = Failure::read("<tool_use_error>String to replace not found in file.</tool_use_error>");

        assert_eq!(failure.label(), "Error");
        assert_eq!(failure.message, "String to replace not found in file.");
    }

    #[test]
    fn output_that_only_looks_like_an_exit_code_is_kept() {
        assert_eq!(output("Exit code soon\nmore"), "Exit code soon\nmore");
        assert_eq!(Failure::read("Exit code 1").message, "");
    }
}
