use protocol::capability::calls::call::{
    CellEdit, Command, CommandPart, Edit, Fetch, FindFiles, LineRange, Monitor, NotebookEdit, Read, Replace, Replaces,
    Runs, Search, Shows, StartSubagent, UseSkill, WebSearch, Write,
};
use protocol::capability::calls::output::{
    CommandOutput, Ended, FileOutcome, Found, FoundLine, Lines, Paths, WebResult,
};
use protocol::capability::calls::{Call, CallOutput};
use protocol::capability::output::{Todo, TodoList, TodoStatus};
use protocol::event::FileChange;
use serde_json::{Value, json};

use super::{call, outputs};

fn command(command: &str, description: Option<&str>, runs: Runs, parts: Vec<CommandPart>) -> Call {
    Call::Command(Command {
        command: command.into(),
        cwd: None,
        description: description.map(Into::into),
        timeout_ms: None,
        runs,
        parts,
        unsandboxed: None,
    })
}

fn replace(old: &str, new: &str, replaces: Replaces) -> Replace {
    Replace { old: old.into(), new: new.into(), replaces }
}

/// That each of `calls`, a tool, its input and the call it makes, makes that call.
fn makes(calls: impl IntoIterator<Item = (&'static str, Value, Call)>) {
    for (tool, input, expected) in calls {
        assert_eq!(call(tool, &input), expected, "{tool} {input}");
    }
}

#[test]
fn each_of_the_cli_s_tools_on_files_and_commands_is_the_call_it_makes() {
    let read_one = vec![CommandPart::Read { path: "a.rs".into() }];
    makes([
        (
            "Bash",
            json!({"command": "cat a.rs", "description": "Look"}),
            command("cat a.rs", Some("Look"), Runs::Waits, read_one),
        ),
        (
            "Bash",
            json!({"command": "make", "run_in_background": true, "timeout": 5000}),
            Call::Command(Command {
                command: "make".into(),
                cwd: None,
                description: None,
                timeout_ms: Some(5000),
                runs: Runs::Background,
                parts: vec![CommandPart::Other],
                unsandboxed: None,
            }),
        ),
        (
            "Read",
            json!({"file_path": "/a.rs", "offset": 10, "limit": 5}),
            Call::Read(Read {
                path: "/a.rs".into(),
                lines: Some(LineRange { first: 10, count: Some(5) }),
                pages: None,
            }),
        ),
        ("Read", json!({"file_path": "/a.rs"}), Call::Read(Read { path: "/a.rs".into(), lines: None, pages: None })),
        (
            "Write",
            json!({"file_path": "/a.rs", "content": "x"}),
            Call::Write(Write { path: "/a.rs".into(), content: "x".into() }),
        ),
        (
            "Edit",
            json!({"file_path": "/a.rs", "old_string": "a", "new_string": "b", "replace_all": true}),
            Call::Edit(Edit { path: "/a.rs".into(), edits: vec![replace("a", "b", Replaces::Every)] }),
        ),
        (
            "MultiEdit",
            json!({"file_path": "/a.rs", "edits": [{"old_string": "a", "new_string": "b"}, {"old_string": "c", "new_string": "d"}]}),
            Call::Edit(Edit {
                path: "/a.rs".into(),
                edits: vec![replace("a", "b", Replaces::One), replace("c", "d", Replaces::One)],
            }),
        ),
        (
            "NotebookEdit",
            json!({"notebook_path": "/n.ipynb", "cell_id": "c1", "new_source": "print(1)", "edit_mode": "insert"}),
            Call::NotebookEdit(NotebookEdit {
                path: "/n.ipynb".into(),
                cell: Some("c1".into()),
                source: "print(1)".into(),
                does: CellEdit::InsertAfter,
            }),
        ),
    ]);
}

#[test]
fn each_of_the_cli_s_tools_that_look_for_something_is_the_call_it_makes() {
    makes([
        (
            "Grep",
            json!({"pattern": "fn a", "path": "src", "glob": "*.rs", "output_mode": "content"}),
            Call::Search(Search {
                pattern: "fn a".into(),
                path: Some("src".into()),
                files: Some("*.rs".into()),
                shows: Shows::Lines,
            }),
        ),
        (
            "Grep",
            json!({"pattern": "fn a"}),
            Call::Search(Search { pattern: "fn a".into(), path: None, files: None, shows: Shows::Files }),
        ),
        ("Glob", json!({"pattern": "**/*.rs"}), Call::FindFiles(FindFiles { pattern: "**/*.rs".into(), path: None })),
        (
            "WebFetch",
            json!({"url": "https://a.dev", "prompt": "What's new?"}),
            Call::Fetch(Fetch { url: "https://a.dev".into(), asking: Some("What's new?".into()) }),
        ),
        (
            "WebSearch",
            json!({"query": "gpui", "allowed_domains": ["zed.dev"]}),
            Call::WebSearch(WebSearch {
                query: "gpui".into(),
                only_domains: vec!["zed.dev".into()],
                not_domains: Vec::new(),
            }),
        ),
    ]);
}

#[test]
fn each_of_the_cli_s_other_tools_is_the_call_it_makes() {
    makes([
        (
            "Agent",
            json!({"description": "Check", "prompt": "Look", "subagent_type": "Explore", "run_in_background": true}),
            Call::Subagent(StartSubagent {
                persona: Some("Explore".into()),
                description: Some("Check".into()),
                prompt: "Look".into(),
                model: None,
                effort: None,
                runs: Runs::Background,
                resumes: None,
            }),
        ),
        (
            "TodoWrite",
            json!({"todos": [{"content": "Test", "status": "in_progress", "activeForm": "Testing"}, {"content": "Ship", "status": "completed"}]}),
            Call::Todos(TodoList {
                items: vec![
                    Todo {
                        id: None,
                        text: "Test".into(),
                        status: TodoStatus::InProgress,
                        doing: Some("Testing".into()),
                        priority: None,
                        after: Vec::new(),
                    },
                    Todo {
                        id: None,
                        text: "Ship".into(),
                        status: TodoStatus::Done,
                        doing: None,
                        priority: None,
                        after: Vec::new(),
                    },
                ],
                note: None,
            }),
        ),
        (
            "Skill",
            json!({"skill": "review", "args": "low"}),
            Call::Skill(UseSkill { name: "review".into(), arguments: Some("low".into()) }),
        ),
        (
            "Monitor",
            json!({"command": "tail -f log", "description": "Watch"}),
            Call::Monitor(Monitor {
                command: "tail -f log".into(),
                description: Some("Watch".into()),
                timeout_ms: None,
            }),
        ),
        ("advisor", json!({}), Call::Advisor),
        ("AskUserQuestion", json!({"questions": []}), Call::Questions),
        ("ExitPlanMode", json!({"plan": "1."}), Call::Plan),
    ]);
}

#[test]
fn any_other_tool_and_any_input_its_kind_doesnt_take_are_told_as_they_are() {
    let other = |tool: &str, input: Value| Call::Other { tool: tool.into(), input };

    assert_eq!(
        call("mcp__github__create_issue", &json!({"title": "x"})),
        Call::Mcp { server: "github".into(), tool: "create_issue".into(), input: json!({"title": "x"}) }
    );
    assert_eq!(call("mcp__odd", &json!({})), other("mcp__odd", json!({})));
    assert_eq!(call("ToolSearch", &json!({"query": "x"})), other("ToolSearch", json!({"query": "x"})));
    assert_eq!(call("Bash", &json!({"cmd": "ls"})), other("Bash", json!({"cmd": "ls"})), "a command it doesn't name");
    assert_eq!(call("Edit", &json!({"file_path": "a"})), other("Edit", json!({"file_path": "a"})));
}

/// What came back for `tool` called with `input`, in the CLI's words `text`, on a line that says `beside`.
fn output(tool: Option<(&str, Value)>, is_error: bool, text: &str, beside: &Value) -> CallOutput {
    let call = tool.map(|(tool, input)| call(tool, &input));
    outputs::of(call.as_ref(), text.into(), is_error, &json!({ "tool_use_result": beside }))
}

fn printed(output: &str, exit_code: Option<i32>) -> CallOutput {
    CallOutput::Command(CommandOutput {
        output: output.into(),
        exit_code,
        ended: Ended::Exited,
        took_ms: None,
        truncated: None,
    })
}

fn words(text: &str) -> CallOutput {
    CallOutput::Words { text: text.into() }
}

#[test]
fn a_failed_command_says_how_it_exited_apart_from_what_it_printed_tags_and_all() {
    let bash = || Some(("Bash", json!({"command": "ls x"})));

    assert_eq!(
        output(bash(), true, "Exit code 2\nls: x: No such file\nmore", &Value::Null),
        printed("ls: x: No such file\nmore", Some(2))
    );
    assert_eq!(output(bash(), true, "Exit code 1", &Value::Null), printed("", Some(1)));
    let tagged = "Exit code 3\n<tool_use_error>its own</tool_use_error>";
    assert_eq!(
        output(bash(), true, tagged, &Value::Null),
        printed("<tool_use_error>its own</tool_use_error>", Some(3))
    );
    assert_eq!(
        output(bash(), false, "Exit code 2\nfine", &Value::Null),
        printed("Exit code 2\nfine", None),
        "only when it failed"
    );
    assert_eq!(output(bash(), true, "Command timed out", &Value::Null), printed("Command timed out", None));
}

#[test]
fn a_tool_s_own_refusal_loses_the_cli_s_tags_and_a_failed_read_is_what_it_said() {
    let refused = "<tool_use_error>File does not exist.</tool_use_error>";
    let read = || Some(("Read", json!({"file_path": "/x"})));

    assert_eq!(output(read(), true, refused, &Value::Null), words("File does not exist."));
    assert_eq!(output(read(), true, "Exit code 2\nno", &Value::Null), words("Exit code 2\nno"), "it isn't a command");
}

#[test]
fn a_result_whose_call_isnt_known_is_read_as_a_failed_command_s_would_be_and_as_words_otherwise() {
    assert_eq!(output(None, true, "Exit code 4\nbad", &Value::Null), printed("bad", Some(4)));
    assert_eq!(
        output(None, true, "<tool_use_error>no such tool</tool_use_error>", &Value::Null),
        words("no such tool")
    );
    assert_eq!(output(None, false, "a.rs", &Value::Null), words("a.rs"));
}

#[test]
fn a_read_is_its_lines_as_the_cli_says_them_beside_its_words_or_in_them() {
    let read = || Some(("Read", json!({"file_path": "/a.rs", "offset": 3})));
    let file = json!({"type": "text", "file": {"filePath": "/a.rs", "content": "c\nd\n", "startLine": 3, "numLines": 2, "totalLines": 9}});
    let lines = |first, lines: &[&str], total| {
        CallOutput::Lines(Lines { first, lines: lines.iter().map(|&l| l.into()).collect(), total })
    };

    assert_eq!(output(read(), false, "     3\tc\n     4\td", &file), lines(3, &["c", "d"], Some(9)));
    let numbered = "     3\tc\n     4→d\n\n<system-reminder>Not the file</system-reminder>";
    assert_eq!(output(read(), false, numbered, &Value::Null), lines(3, &["c", "d"], None));
    let image = json!({"type": "image", "file": {"base64": "AAAA"}});
    assert_eq!(output(read(), false, "", &image), words(""), "an image is the result's");
}

#[test]
fn a_search_is_what_its_words_say_it_found() {
    let grep = || Some(("Grep", json!({"pattern": "fn", "output_mode": "content"})));
    let glob = || Some(("Glob", json!({"pattern": "*.rs"})));
    let files = |paths: &[&str], truncated| {
        CallOutput::Found(Found::Files(Paths { paths: paths.iter().map(|&p| p.into()).collect(), truncated }))
    };
    let line = |path: &str, line, text: &str| FoundLine { path: path.into(), line, text: text.into() };

    assert_eq!(
        output(grep(), false, "src/a.rs:3:fn a() {\nsrc/c.rs:1:fn c() {", &Value::Null),
        CallOutput::Found(Found::Lines {
            lines: vec![line("src/a.rs", 3, "fn a() {"), line("src/c.rs", 1, "fn c() {")],
            truncated: false
        })
    );
    assert_eq!(
        output(grep(), false, "Found 2 files\nsrc/a.rs\nsrc/c.rs", &Value::Null),
        files(&["src/a.rs", "src/c.rs"], false)
    );
    let cut = "/w/a.rs\n/w/b.rs\n(Results are truncated. Consider using a more specific path or pattern.)";
    assert_eq!(output(glob(), false, cut, &Value::Null), files(&["/w/a.rs", "/w/b.rs"], true));
    assert_eq!(output(glob(), false, "No files found", &Value::Null), files(&[], false));
    assert_eq!(output(grep(), false, "No matches found", &Value::Null), files(&[], false));
    let around = "src/a.rs-2-// before\nsrc/a.rs:3:fn a() {";
    assert_eq!(output(grep(), false, around, &Value::Null), words(around), "lines around a match are words");
}

#[test]
fn a_web_search_is_the_pages_it_found_and_what_was_made_of_them() {
    let search = || Some(("WebSearch", json!({"query": "gpui"})));
    let said = "Web search results for query: \"gpui\"\n\nLinks: [{\"title\":\"GPUI\",\"url\":\"https://gpui.rs\"},{\"url\":\"https://zed.dev\"}]\n\nGPUI is Zed's UI framework.\n\nREMINDER: cite the links.";
    let page = |title: &str, url: &str| WebResult { title: title.into(), url: url.into(), snippet: None };

    assert_eq!(
        output(search(), false, said, &Value::Null),
        CallOutput::WebResults {
            results: vec![page("GPUI", "https://gpui.rs"), page("https://zed.dev", "https://zed.dev")],
            summary: Some("GPUI is Zed's UI framework.".into()),
        }
    );
    assert_eq!(
        output(search(), false, "Web search results for query: \"x\"\n\nNothing useful.", &Value::Null),
        CallOutput::WebResults { results: Vec::new(), summary: Some("Nothing useful.".into()) }
    );
}

#[test]
fn a_write_or_an_edit_says_what_it_changed_as_the_cli_says_beside_its_words() {
    let edit = || Some(("Edit", json!({"file_path": "/a.rs", "old_string": "a", "new_string": "b"})));
    let patch = json!({"structuredPatch": [{"oldStart": 1, "newStart": 1, "lines": ["-a", "+b"]}]});
    let created = json!({"type": "create", "structuredPatch": []});
    let changed = |change| CallOutput::Changed {
        files: vec![FileOutcome { path: "/a.rs".into(), change, moved_to: None, diagnostics: Vec::new() }],
    };

    let CallOutput::Changed { files } = output(edit(), false, "The file /a.rs has been updated.", &patch) else {
        panic!()
    };
    assert!(matches!(files[0].change, Some(FileChange::Changed { .. })), "{files:?}");
    let write = Some(("Write", json!({"file_path": "/a.rs", "content": "b"})));
    assert_eq!(output(write, false, "File created", &created), changed(Some(FileChange::Created)));
    assert_eq!(
        output(edit(), false, "Updated", &Value::Null),
        changed(None),
        "a change it didn't say is left to the call"
    );
}

#[test]
fn a_fetch_is_its_page_a_subagent_its_report_and_an_mcp_tool_what_it_said() {
    let fetch = Some(("WebFetch", json!({"url": "https://a.dev", "prompt": "What?"})));
    let agent = Some(("Agent", json!({"prompt": "Look"})));
    let mcp = || Some(("mcp__github__get", json!({})));

    assert_eq!(
        output(fetch, false, "It's new.", &json!({"code": 200})),
        CallOutput::Page { url: "https://a.dev".into(), status: Some(200), text: "It's new.".into() }
    );
    assert_eq!(output(agent.clone(), false, "Done.", &Value::Null), CallOutput::Report { report: "Done.".into() });
    let launched = "Async agent launched successfully.\nagentId: a1 (internal ID - do not mention to user.)";
    assert_eq!(output(agent, false, launched, &Value::Null), CallOutput::Backgrounded { task: None });
    let CallOutput::Mcp { content, .. } = output(mcp(), false, "an issue", &Value::Null) else { panic!() };
    assert_eq!(content.len(), 1);
    assert_eq!(output(mcp(), false, "", &Value::Null), CallOutput::Mcp { content: Vec::new(), structured: None });
    assert_eq!(output(Some(("ToolSearch", json!({}))), false, "found 2", &Value::Null), words("found 2"));
}
