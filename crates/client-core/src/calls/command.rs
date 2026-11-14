//! A shell command, and what the agent said it's for.

use serde_json::Value;

use super::{description, first_line, str_field};

pub struct Command<'a> {
    /// What the agent said it's doing, in its own words.
    pub(super) description: Option<&'a str>,
    pub command: &'a str,
}

impl<'a> Command<'a> {
    pub(super) fn read(input: &'a Value) -> Option<Self> {
        let command = str_field(input, "command")?;
        Some(Self { description: description(input), command })
    }

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
    use serde_json::json;

    use super::Command;

    #[test]
    fn the_title_is_what_the_agent_said_it_is_doing() {
        let input = json!({ "command": "cargo test\n  --workspace", "description": "Run the tests" });

        let command = Command::read(&input).unwrap();

        assert_eq!(command.title(), "Run the tests");
        assert_eq!(command.summary(), "cargo test");
    }

    #[test]
    fn a_command_with_no_description_says_it_runs_something() {
        for input in [json!({ "command": "ls" }), json!({ "command": "ls", "description": "  " })] {
            assert_eq!(Command::read(&input).unwrap().title(), "Run");
        }
    }
}
