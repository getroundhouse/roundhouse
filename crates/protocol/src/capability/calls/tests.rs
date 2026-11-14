//! Every kind of call, and of what one sends back, read back as it was written, with each of its optional parts set,
//! since a part named as its kind's tag would collide with it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::call::*;
use super::output::*;
use super::*;
use crate::capability::approvals::{Asks, Request};
use crate::capability::output::{Todo, TodoList, TodoStatus};
use crate::capability::settings::Effort;
use crate::event::{FileChange, HunkLine, LineMark};
use crate::model::ModelName;

fn reads_back<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(value: &T) {
    let text = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&text).unwrap(), value, "{text}");
}

fn files() -> Vec<Call> {
    let replace = Replace { old: "a".into(), new: "b".into(), replaces: Replaces::Every };
    let line = HunkLine { mark: LineMark::Added, text: "    run();".into() };
    let hunk = PatchHunk { after: Some("fn main() {".into()), lines: vec![line] };
    let update = PatchChange::Update { move_to: Some("b.rs".into()), hunks: vec![hunk] };
    vec![
        Call::Read(Read {
            path: "a".into(),
            lines: Some(LineRange { first: 2, count: Some(3) }),
            pages: Some("1-5".into()),
        }),
        Call::Write(Write { path: "a".into(), content: "x".into() }),
        Call::Edit(Edit { path: "a".into(), edits: vec![replace] }),
        Call::Patch(Patch { files: vec![FilePatch { path: "a.rs".into(), change: update }] }),
        Call::NotebookEdit(NotebookEdit {
            path: "n.ipynb".into(),
            cell: Some("c1".into()),
            source: "x = 1".into(),
            does: CellEdit::InsertAfter,
        }),
        Call::Search(Search {
            pattern: "fn".into(),
            path: Some("src".into()),
            files: Some("*.rs".into()),
            shows: Shows::Count,
        }),
        Call::FindFiles(FindFiles { pattern: "**/*.rs".into(), path: Some("src".into()) }),
    ]
}

fn commands() -> Vec<Call> {
    let parts = vec![
        CommandPart::Read { path: "a".into() },
        CommandPart::List { path: Some("src".into()) },
        CommandPart::Search { pattern: Some("fn".into()), path: None },
        CommandPart::Other,
    ];
    vec![
        Call::Command(Command {
            command: "cat a".into(),
            cwd: Some("src".into()),
            description: Some("Read a".into()),
            timeout_ms: Some(1000),
            runs: Runs::Background,
            parts,
            unsandboxed: Some("it writes outside".into()),
        }),
        Call::WriteInput(WriteInput { command: "t".into(), text: "y\n".into() }),
        Call::Monitor(Monitor {
            command: "tail -f log".into(),
            description: Some("Watch".into()),
            timeout_ms: Some(5),
        }),
        Call::Sleep(Sleep { duration_ms: 10 }),
        Call::Schedule(Schedule {
            prompt: "check".into(),
            when: When::Cron { expression: "0 * * * *".into(), repeats: true },
        }),
        Call::Schedule(Schedule { prompt: "check".into(), when: When::After { seconds: 60 } }),
    ]
}

fn web_and_subagents() -> Vec<Call> {
    let todo = Todo {
        id: Some("1".into()),
        text: "Test".into(),
        status: TodoStatus::InProgress,
        doing: Some("Testing".into()),
        priority: None,
        after: vec!["0".into()],
    };
    vec![
        Call::Fetch(Fetch { url: "https://example.com".into(), asking: Some("What's new?".into()) }),
        Call::WebSearch(WebSearch { query: "rust".into(), only_domains: vec!["a.com".into()], not_domains: vec![] }),
        Call::FindInPage(FindInPage { url: "https://example.com".into(), pattern: "x".into() }),
        Call::Todos(TodoList { items: vec![todo], note: Some("Why".into()) }),
        Call::Subagent(StartSubagent {
            persona: Some("Explore".into()),
            description: Some("Look".into()),
            prompt: "Find it".into(),
            model: Some(ModelName::named("haiku")),
            effort: Some(Effort::Low),
            runs: Runs::Background,
            resumes: Some("t0".into()),
        }),
        Call::MessageSubagent(MessageSubagent { subagent: "t".into(), message: "and this".into() }),
        Call::WaitForSubagents(WaitForSubagents { subagents: vec!["t".into()], timeout_ms: Some(5) }),
        Call::StopSubagent(StopSubagent { subagent: "t".into() }),
    ]
}

fn others() -> Vec<Call> {
    let at = Some(Position { line: 1, column: 2 });
    vec![
        Call::ViewImage(ViewImage { path: "a.png".into() }),
        Call::GenerateImage(GenerateImage { prompt: "a cat".into(), from: vec!["b.png".into()] }),
        Call::Skill(UseSkill { name: "review".into(), arguments: Some("--fast".into()) }),
        Call::Lsp(Lsp { asks: LspQuestion::References, path: "a.rs".into(), at }),
        Call::Advisor,
        Call::Workflow(Workflow { name: Some("release".into()) }),
        Call::Questions,
        Call::Plan,
        Call::Mcp { server: "roundhouse".into(), tool: "wait".into(), input: json!({"kind": "x"}) },
        Call::Other { tool: "Mystery".into(), input: json!({"kind": "x"}) },
    ]
}

#[test]
fn every_kind_of_call_reads_back_as_it_was_written() {
    let calls: Vec<Call> = [files(), commands(), web_and_subagents(), others()].concat();
    for call in &calls {
        reads_back(call);
        reads_back(&Request { asks: Asks::Call { call: call.clone() }, suggested: vec![], always: true, why: None });
    }

    let kinds: HashSet<CallKind> = calls.iter().map(Call::kind).collect();
    assert_eq!(kinds.len(), 30, "one of every kind but `Unknown`");
    assert_eq!(serde_json::from_value::<Call>(json!({"kind": "telepathy", "mind": "x"})).unwrap(), Call::Unknown);
}

#[test]
fn a_call_is_known_unless_it_holds_something_this_build_doesnt_know() {
    let calls: Vec<Call> = [files(), commands(), web_and_subagents(), others()].concat();
    // The call of `kind` with its value at `pointer` one only a newer build would send.
    let holding = |kind: CallKind, pointer: &str| {
        let mut value = serde_json::to_value(calls.iter().find(|call| call.kind() == kind).unwrap()).unwrap();
        *value.pointer_mut(pointer).unwrap() = json!("from_a_newer_build");
        serde_json::from_value::<Call>(value).unwrap()
    };

    assert!(calls.iter().all(Call::is_known));
    assert!(!Call::Unknown.is_known());
    for (kind, pointer) in [
        (CallKind::Command, "/runs"),
        (CallKind::Edit, "/edits/0/replaces"),
        (CallKind::Patch, "/files/0/change/change"),
        (CallKind::Patch, "/files/0/change/hunks/0/lines/0/mark"),
        (CallKind::NotebookEdit, "/does"),
        (CallKind::Search, "/shows"),
        (CallKind::Todos, "/items/0/status"),
        (CallKind::Subagent, "/runs"),
        (CallKind::Subagent, "/effort"),
        (CallKind::Lsp, "/asks"),
        (CallKind::Schedule, "/when/when"),
    ] {
        let call = holding(kind, pointer);
        assert!(!call.is_known(), "{pointer} of {call:?}");
    }
}

#[test]
fn every_kind_of_output_reads_back_as_it_was_written() {
    let file = FileOutcome {
        path: "a".into(),
        change: Some(FileChange::Created),
        moved_to: Some("b".into()),
        diagnostics: vec![Diagnostic { severity: Severity::Hint, line: 1, column: Some(2), message: "x".into() }],
    };
    let ended =
        |ended| CommandOutput { output: "ok".into(), exit_code: Some(0), ended, took_ms: Some(1), truncated: None };
    let outputs = [
        CallOutput::Command(ended(Ended::Backgrounded { task: "t".into() })),
        CallOutput::Command(ended(Ended::Exited)),
        CallOutput::Lines(Lines { first: 1, lines: vec!["fn main() {}".into()], total: Some(1) }),
        CallOutput::Document { pages: Some(2) },
        CallOutput::Changed { files: vec![file] },
        CallOutput::Found(Found::Lines { lines: vec![], truncated: true }),
        CallOutput::Paths { paths: vec!["a".into()], truncated: true },
        CallOutput::Page { url: "https://example.com".into(), status: Some(200), text: "hi".into() },
        CallOutput::WebResults {
            results: vec![WebResult { title: "a".into(), url: "b".into(), snippet: Some("c".into()) }],
        },
        CallOutput::Report { report: "done".into() },
        CallOutput::Mcp { content: vec![McpContent::Text { text: "x".into() }], structured: Some(json!({"kind": 1})) },
        CallOutput::Words { text: "ok".into() },
    ];
    for output in &outputs {
        reads_back(output);
    }
    for output in more_outputs() {
        reads_back(&output);
    }

    let command = serde_json::to_value(ended(Ended::Exited)).unwrap();
    assert_eq!(command["ended"], json!("exited"), "how it ended is beside what it printed");
}

fn more_outputs() -> Vec<CallOutput> {
    let image: crate::attachment::Attachment =
        serde_json::from_value(json!({"id": "a", "name": "a.png", "media_type": "image/png", "size_bytes": 1}))
            .unwrap();
    let truncated = Some(Truncated { full_at: Some("/tmp/out".into()) });
    let printed =
        CommandOutput { output: "…".into(), exit_code: None, ended: Ended::TimedOut, took_ms: None, truncated };
    vec![
        CallOutput::Image { image: image.clone(), prompt: Some("a cat".into()) },
        CallOutput::Command(printed),
        CallOutput::Found(Found::Files { paths: vec!["a".into()], truncated: true }),
        CallOutput::Found(Found::Count { matches: 3, files: 1 }),
        CallOutput::Mcp {
            content: vec![
                McpContent::Image { image },
                McpContent::Resource { uri: "file:///a".into(), text: Some("x".into()) },
            ],
            structured: None,
        },
    ]
}

#[test]
fn a_patch_that_adds_or_deletes_a_file_reads_back() {
    let patch = Call::Patch(Patch {
        files: vec![
            FilePatch { path: "new.rs".into(), change: PatchChange::Add { content: "fn f() {}".into() } },
            FilePatch { path: "old.rs".into(), change: PatchChange::Delete },
        ],
    });

    reads_back(&patch);
}
