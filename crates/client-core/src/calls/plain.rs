//! What the typed calls a client draws as any typed call say: each kind's title in words, and its most telling input,
//! as any other tool's are.

use protocol::capability::calls::call::{Lsp, LspQuestion, Monitor, Sleep, WriteInput};

use super::{count, first_line};

/// Writing to a command that's still running, or with nothing to write, reading what it has printed since.
pub(super) fn write_input_title(input: &WriteInput) -> &'static str {
    if input.text.is_empty() { "Read a command's output" } else { "Write to a command" }
}

/// What the language server is asked.
pub(super) fn lsp_title(lsp: &Lsp) -> &'static str {
    match lsp.asks {
        LspQuestion::Definition => "Find a definition",
        LspQuestion::References => "Find references",
        LspQuestion::Implementation => "Find implementations",
        LspQuestion::Hover => "Look up a symbol",
        LspQuestion::Symbols => "List a file's symbols",
        LspQuestion::WorkspaceSymbols => "Find symbols",
        LspQuestion::Callers => "Find callers",
        LspQuestion::Callees => "Find callees",
        LspQuestion::Unknown => "Ask the language server",
    }
}

/// The file it's about, and where in it, as a compiler says a place: `src/a.rs:3:5`.
pub(super) fn lsp_place(lsp: &Lsp) -> String {
    match lsp.at {
        Some(at) => format!("{}:{}:{}", lsp.path, at.line, at.column),
        None => lsp.path.clone(),
    }
}

/// How long, in its largest whole unit, such as `20 minutes`, `90 seconds` or `1500 milliseconds`.
pub(super) fn how_long(sleep: Sleep) -> String {
    let (ms, seconds) = (sleep.duration_ms, sleep.duration_ms / 1000);
    if ms % 1000 != 0 || seconds == 0 {
        count(usize_of(ms), "millisecond", "milliseconds")
    } else if seconds % 3600 == 0 {
        count(usize_of(seconds / 3600), "hour", "hours")
    } else if seconds % 60 == 0 {
        count(usize_of(seconds / 60), "minute", "minutes")
    } else {
        count(usize_of(seconds), "second", "seconds")
    }
}

/// What a monitor watches for, as the agent said, or else the command it watches.
pub(super) fn watching(monitor: &Monitor) -> String {
    let said = monitor.description.as_deref().map(str::trim).filter(|said| !said.is_empty());
    first_line(said.unwrap_or(&monitor.command)).to_string()
}

fn usize_of(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::Call;
    use protocol::capability::calls::call::{
        FindInPage, Lsp, LspQuestion, Monitor, Position, Schedule, Sleep, UseSkill, When, Workflow, WriteInput,
    };
    use protocol::capability::output::{Todo, TodoList, TodoStatus};

    use crate::calls::{Recorded, ToolCall, typed};

    /// Each kind a client draws as any typed call is titled in words, not by its kind's name, and says its most
    /// telling input, as the CLI prints them, or nothing where it has none.
    #[test]
    fn every_other_kind_of_typed_call_is_said_in_words() {
        let said = |call: Call| (typed::title(&call), Recorded::Typed(&call).summary());
        let words = |title: &str, about: Option<&str>| (title.to_string(), about.map(str::to_string));
        let schedule = Schedule { prompt: "Check CI on #182\nand say".into(), when: When::After { seconds: 1200 } };
        let todo = |text: &str| Todo {
            id: None,
            text: text.into(),
            status: TodoStatus::Pending,
            doing: None,
            priority: None,
            after: Vec::new(),
        };
        let lsp =
            Lsp { asks: LspQuestion::References, path: "src/a.rs".into(), at: Some(Position { line: 3, column: 5 }) };
        let monitor = |description: Option<&str>| Monitor {
            command: "tail -f test.log".into(),
            description: description.map(Into::into),
            timeout_ms: None,
        };
        let input = |text: &str| WriteInput { command: "run-1".into(), text: text.into() };
        let sleep = |duration_ms| Call::Sleep(Sleep { duration_ms });

        assert_eq!(said(Call::Schedule(schedule)), words("Schedule a prompt", Some("Check CI on #182")));
        assert_eq!(
            said(Call::Workflow(Workflow { name: Some("release".into()) })),
            words("Run a workflow", Some("release"))
        );
        assert_eq!(said(Call::Workflow(Workflow { name: None })), words("Run a workflow", None));
        let skill = UseSkill { name: "release-notes".into(), arguments: None };
        assert_eq!(said(Call::Skill(skill)), words("Use a skill", Some("release-notes")));
        let list = TodoList { items: vec![todo("Test"), todo("Ship")], note: None };
        assert_eq!(said(Call::Todos(list)), words("Update the todo list", Some("2 todos")));
        assert_eq!(said(Call::Lsp(lsp)), words("Find references", Some("src/a.rs:3:5")));
        assert_eq!(
            said(Call::Monitor(monitor(Some(" Watch for failures")))),
            words("Monitor", Some("Watch for failures"))
        );
        assert_eq!(said(Call::Monitor(monitor(None))).1.as_deref(), Some("tail -f test.log"), "what it watches");
        assert_eq!(said(Call::WriteInput(input("y\n"))), words("Write to a command", Some("y")));
        assert_eq!(said(Call::WriteInput(input(""))), words("Read a command's output", None));
        let find = FindInPage { url: "https://example.com".into(), pattern: "may_pass".into() };
        assert_eq!(said(Call::FindInPage(find)), words("Find in a page", Some("may_pass")));
        let waited: Vec<_> = [1_200_000, 90_000, 7_200_000, 1500].map(|ms| said(sleep(ms)).1.unwrap()).to_vec();
        assert_eq!(waited, ["20 minutes", "90 seconds", "2 hours", "1500 milliseconds"]);
        assert_eq!(said(Call::Questions), words("Ask questions", None));
        assert!(matches!(ToolCall::read(Recorded::Typed(&Call::Advisor)), ToolCall::Advisor), "drawn as Claude's is");
    }
}
