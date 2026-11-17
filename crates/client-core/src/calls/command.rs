//! A shell command, and what the agent said it's for.

use super::first_line;
use super::search::Search;

pub struct Command<'a> {
    /// What the agent said it's doing, in its own words.
    pub(super) description: Option<&'a str>,
    pub command: &'a str,
    /// What it does and nothing else, where its agent read the command and said so, which a client can draw as that
    /// kind of call. What's asked of a command, though, is still whether to run it.
    pub only: Option<Only<'a>>,
}

/// All a command does, where that's a kind of call of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Only<'a> {
    /// Reads this one file.
    Reads(&'a str),
    /// Searches what's in files, as one search.
    Searches(Search<'a>),
}

impl<'a> Command<'a> {
    /// What its line says it does: the agent's own words when it gave them.
    pub fn title(&self) -> &'a str {
        self.description.unwrap_or("Run")
    }

    /// What the agent said it's doing, if it said, which is the title then.
    pub fn described(&self) -> Option<&'a str> {
        self.description
    }

    /// The command on one line, for beside the title.
    pub fn summary(&self) -> &'a str {
        first_line(self.command)
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::Call;
    use protocol::fixtures;

    use super::super::{Recorded, ToolCall};

    /// Running `command`, which the agent said is for `description`, as it's drawn: its title, and what it's about.
    fn drawn(command: &str, description: Option<&str>) -> (String, String) {
        let typed = protocol::capability::calls::call::Command {
            description: description.map(Into::into),
            ..fixtures::typed_command(command)
        };
        let call = Call::Command(typed);
        let ToolCall::Command(command) = ToolCall::read(Recorded::Typed(&call)) else { panic!("not a command") };
        (command.title().to_string(), command.summary().to_string())
    }

    #[test]
    fn the_title_is_what_the_agent_said_it_is_doing() {
        assert_eq!(
            drawn("cargo test\n  --workspace", Some("Run the tests")),
            ("Run the tests".into(), "cargo test".into())
        );
    }

    #[test]
    fn a_command_with_no_description_says_it_runs_something() {
        for description in [None, Some("  ")] {
            assert_eq!(drawn("ls", description).0, "Run");
        }
    }
}
