//! What a client says of a background task, on one line, so the app and the command line say the same.

use protocol::event::AgentTask;
use protocol::short_text::one_line;

use crate::calls::{UNDESCRIBED, first_line, tool_name};
use crate::tool_servers::ToolServerNames;

/// More characters than a line has room for, so a long command isn't copied to be cut off.
const MOST: usize = 240;

/// What a task's line says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Says {
    /// What it's for, in words: the agent's, or the app's own, as of a wait or a call to an MCP server's tool.
    For(String),
    /// The first line of the command it runs, since the agent didn't say what that's for, as a command's own line says.
    Runs(String),
}

impl Says {
    /// What `task`'s line says, or `None` for one its agent said nothing of. A call to an MCP server's tool says the
    /// server, as `servers` shows it, then the tool's name made readable. One from an agent, or a log, that doesn't say
    /// which tool it's of says what its agent said, with the name its agent calls the server by.
    pub fn of(task: &AgentTask, servers: &ToolServerNames) -> Option<Self> {
        let on_one_line = |text: &str| Some(one_line(text, MOST)).filter(|line| !line.is_empty());
        if let Some(of) = &task.tool {
            let called = format!("{}: {}", servers.shown(&of.server), tool_name(&of.tool));
            return on_one_line(&called).map(Says::For);
        }
        let said = task.description.as_deref().and_then(on_one_line).map(Says::For);
        said.or_else(|| task.command.as_deref().map(first_line).and_then(on_one_line).map(Says::Runs))
    }

    /// It in words alone, where the command isn't set apart as code.
    pub fn into_words(self) -> String {
        match self {
            Says::For(what) => what,
            Says::Runs(command) => format!("{UNDESCRIBED} {command}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{AgentTask, TaskKind, TaskTool};
    use protocol::fixtures;
    use protocol::model::{ToolServer, ToolServerName};

    use super::{MOST, Says};
    use crate::tool_servers::ToolServerNames;

    /// Every description and command a task can have, however many lines it's written on, is one line.
    #[test]
    fn a_task_says_what_its_for_or_else_the_first_line_of_its_command_and_always_on_one_line() {
        let says = |description: Option<&str>, command: Option<&str>| {
            let (description, command) = (description.map(Into::into), command.map(Into::into));
            let task = AgentTask { description, command, ..AgentTask::new("t", TaskKind::Command) };
            Says::of(&task, &ToolServerNames::default())
        };
        let script = "\npython3 - <<'EOF'\nprint('done')\nEOF";

        assert_eq!(says(Some("Run the tests"), None), Some(Says::For("Run the tests".into())));
        assert_eq!(says(None, Some(script)), Some(Says::Runs("python3 - <<'EOF'".into())));
        let recorded_before = Some(Says::For("python3 - <<'EOF' print('done') EOF".into()));
        assert_eq!(says(Some(script), None), recorded_before, "a list that gives the command as its description");
        assert_eq!(says(Some(" \n"), Some("ls")), Some(Says::Runs("ls".into())), "a description that says nothing");
        assert_eq!((says(None, None), says(None, Some("\n"))), (None, None));
        let Some(Says::Runs(long)) = says(None, Some(&"x".repeat(MOST * 2))) else { panic!("a command") };
        assert_eq!(long.len(), MOST);
        assert_eq!(Says::Runs("ls".into()).into_words(), "Run ls");
    }

    /// A task from an older worker, or in a log it wrote, says only its description.
    #[test]
    fn a_task_of_a_servers_tool_says_the_persons_name_for_the_server_and_any_other_what_its_agent_said() {
        let northwind =
            ToolServer { name: ToolServerName::stored("Linear (Northwind)".into()), ..fixtures::tool_server("t") };
        let names = ToolServerNames::of(&[northwind]);
        let of = |server: &str, tool: &str| Some(TaskTool { server: server.into(), tool: tool.into() });
        let said = |description: Option<&str>, tool| {
            let description = description.map(Into::into);
            let task = AgentTask { description, tool, ..AgentTask::new("t", TaskKind::Command) };
            Says::of(&task, &names).map(Says::into_words)
        };
        let agents = Some("Linear_Northwind/export_issues");

        let exporting = said(agents, of("Linear_Northwind", "export_issues"));
        assert_eq!(exporting.as_deref(), Some("Linear (Northwind): Export issues"));
        let cli_s_own = said(Some("docs/findPage"), of("docs", "findPage"));
        assert_eq!(cli_s_own.as_deref(), Some("docs: Find page"));
        assert_eq!(
            said(Some("docs/"), of("docs", "")).as_deref(),
            Some("docs: Unknown tool"),
            "as its call's line says"
        );
        let long = said(None, of("docs", &"x".repeat(MOST * 2))).unwrap();
        assert_eq!(long.len(), MOST);
        assert_eq!(said(agents, None).as_deref(), agents);
        assert_eq!(said(Some("Run the tests"), None).as_deref(), Some("Run the tests"));
        assert_eq!(said(None, None), None);
    }
}
