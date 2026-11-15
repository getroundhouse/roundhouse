//! Every call and every output a call sends back reads back as it was written, as a block of a message and in a
//! subagent's record, whatever each holds. A new kind of call doesn't compile here until it has a strategy.

use proptest::collection::vec;
use proptest::option::of;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, RngSeed};
use serde_json::{Value, json};

use super::call::*;
use super::output::*;
use super::{Call, CallKind, CallOutput};
use crate::attachment::Attachment;
use crate::capability::output::{Priority, Todo, TodoList, TodoStatus};
use crate::capability::settings::Effort;
use crate::event::{ContentBlock, Denied, FileChange, Hunk, HunkLine, LineMark};
use crate::model::{LatelyCall, LatelyResult, ModelName, Said};

/// Every kind but `Unknown`, which reads from any kind this build doesn't know.
const KINDS: [CallKind; 30] = [
    CallKind::Command,
    CallKind::WriteInput,
    CallKind::Read,
    CallKind::Write,
    CallKind::Edit,
    CallKind::Patch,
    CallKind::NotebookEdit,
    CallKind::Search,
    CallKind::FindFiles,
    CallKind::Fetch,
    CallKind::WebSearch,
    CallKind::FindInPage,
    CallKind::Todos,
    CallKind::Subagent,
    CallKind::MessageSubagent,
    CallKind::WaitForSubagents,
    CallKind::StopSubagent,
    CallKind::ViewImage,
    CallKind::GenerateImage,
    CallKind::Skill,
    CallKind::Lsp,
    CallKind::Advisor,
    CallKind::Sleep,
    CallKind::Monitor,
    CallKind::Schedule,
    CallKind::Workflow,
    CallKind::Questions,
    CallKind::Plan,
    CallKind::Mcp,
    CallKind::Other,
];
// A kind missing from the list, which `Unknown` comes after, would never be drawn.
const _: () = assert!(KINDS.len() == CallKind::Unknown as usize);

fn text() -> impl Strategy<Value = String> {
    ".{0,12}"
}

fn name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,10}"
}

/// Including keys named as the tags around it are, which mustn't be taken for them.
fn value() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        text().prop_map(Value::String),
        (select(["kind", "type", "found", "ended", "part"].as_slice()), text())
            .prop_map(|(key, text)| json!({ key: text })),
        (name(), text()).prop_map(|(key, text)| json!({ key: text })),
    ]
}

fn image() -> impl Strategy<Value = Attachment> {
    (name(), text(), any::<u32>()).prop_map(|(id, name, size)| {
        serde_json::from_value(json!({"id": id, "name": name, "media_type": "image/png", "size_bytes": size})).unwrap()
    })
}

fn runs() -> impl Strategy<Value = Runs> {
    select([Runs::Waits, Runs::Background, Runs::Terminal].as_slice())
}

fn effort() -> impl Strategy<Value = Effort> {
    use Effort::*;
    select([None, Minimal, Low, Medium, High, Xhigh, Max, Ultra, Persistent].as_slice())
}

fn command() -> impl Strategy<Value = Command> {
    let part = prop_oneof![
        name().prop_map(|path| CommandPart::Read { path }),
        of(name()).prop_map(|path| CommandPart::List { path }),
        (of(text()), of(name())).prop_map(|(pattern, path)| CommandPart::Search { pattern, path }),
        Just(CommandPart::Other),
    ];
    (text(), of(name()), of(text()), of(any::<u64>()), runs(), vec(part, 0..3), of(text())).prop_map(
        |(command, cwd, description, timeout_ms, runs, parts, unsandboxed)| Command {
            command,
            cwd,
            description,
            timeout_ms,
            runs,
            parts,
            unsandboxed,
        },
    )
}

fn hunk_lines() -> impl Strategy<Value = Vec<HunkLine>> {
    let mark = select([LineMark::Same, LineMark::Removed, LineMark::Added].as_slice());
    vec((mark, text()).prop_map(|(mark, text)| HunkLine { mark, text }), 0..3)
}

fn todo() -> impl Strategy<Value = Todo> {
    let status =
        select([TodoStatus::Pending, TodoStatus::InProgress, TodoStatus::Done, TodoStatus::Cancelled].as_slice());
    let priority = of(select([Priority::High, Priority::Medium, Priority::Low].as_slice()));
    (of(name()), text(), status, of(text()), priority, vec(name(), 0..2)).prop_map(
        |(id, text, status, doing, priority, after)| Todo {
            id: id.map(Into::into),
            text,
            status,
            doing,
            priority,
            after: Vec::from_iter(after.into_iter().map(Into::into)),
        },
    )
}

/// A call of `kind`, whatever it holds.
#[expect(clippy::too_many_lines, reason = "one arm for each kind of call, so a new kind has to have one")]
fn call_of(kind: CallKind) -> BoxedStrategy<Call> {
    match kind {
        CallKind::Command => command().prop_map(Call::Command).boxed(),
        CallKind::WriteInput => (name(), text())
            .prop_map(|(command, text)| Call::WriteInput(WriteInput { command: command.into(), text }))
            .boxed(),
        CallKind::Read => {
            let lines = (any::<u32>(), of(any::<u32>())).prop_map(|(first, count)| LineRange { first, count });
            (name(), of(lines), of(text()))
                .prop_map(|(path, lines, pages)| Call::Read(Read { path, lines, pages }))
                .boxed()
        }
        CallKind::Write => (name(), text()).prop_map(|(path, content)| Call::Write(Write { path, content })).boxed(),
        CallKind::Edit => {
            let replaces = select([Replaces::One, Replaces::Every].as_slice());
            let replace = (text(), text(), replaces).prop_map(|(old, new, replaces)| Replace { old, new, replaces });
            (name(), vec(replace, 0..3)).prop_map(|(path, edits)| Call::Edit(Edit { path, edits })).boxed()
        }
        CallKind::Patch => {
            let hunks = vec((of(text()), hunk_lines()).prop_map(|(after, lines)| PatchHunk { after, lines }), 0..2);
            let change = prop_oneof![
                text().prop_map(|content| PatchChange::Add { content }),
                Just(PatchChange::Delete),
                (of(name()), hunks).prop_map(|(move_to, hunks)| PatchChange::Update { move_to, hunks }),
            ];
            vec((name(), change).prop_map(|(path, change)| FilePatch { path, change }), 0..3)
                .prop_map(|files| Call::Patch(Patch { files }))
                .boxed()
        }
        CallKind::NotebookEdit => {
            let does = select([CellEdit::Replace, CellEdit::InsertAfter, CellEdit::Delete].as_slice());
            (name(), of(name()), text(), does)
                .prop_map(|(path, cell, source, does)| {
                    Call::NotebookEdit(NotebookEdit { path, cell: cell.map(Into::into), source, does })
                })
                .boxed()
        }
        CallKind::Search => {
            let shows = select([Shows::Lines, Shows::Files, Shows::Count].as_slice());
            (text(), of(name()), of(text()), shows)
                .prop_map(|(pattern, path, files, shows)| Call::Search(Search { pattern, path, files, shows }))
                .boxed()
        }
        CallKind::FindFiles => {
            (text(), of(name())).prop_map(|(pattern, path)| Call::FindFiles(FindFiles { pattern, path })).boxed()
        }
        CallKind::Fetch => (text(), of(text())).prop_map(|(url, asking)| Call::Fetch(Fetch { url, asking })).boxed(),
        CallKind::WebSearch => (text(), vec(name(), 0..2), vec(name(), 0..2))
            .prop_map(|(query, only_domains, not_domains)| {
                Call::WebSearch(WebSearch { query, only_domains, not_domains })
            })
            .boxed(),
        CallKind::FindInPage => {
            (text(), text()).prop_map(|(url, pattern)| Call::FindInPage(FindInPage { url, pattern })).boxed()
        }
        CallKind::Todos => {
            (vec(todo(), 0..3), of(text())).prop_map(|(items, note)| Call::Todos(TodoList { items, note })).boxed()
        }
        CallKind::Subagent => {
            let model = of(name().prop_filter_map("a model's name", |name| ModelName::try_from(name).ok()));
            (of(name()), of(text()), text(), model, of(effort()), runs(), of(name()))
                .prop_map(|(persona, description, prompt, model, effort, runs, resumes)| {
                    let (persona, resumes) = (persona.map(Into::into), resumes.map(Into::into));
                    Call::Subagent(StartSubagent { persona, description, prompt, model, effort, runs, resumes })
                })
                .boxed()
        }
        CallKind::MessageSubagent => (name(), text())
            .prop_map(|(subagent, message)| {
                Call::MessageSubagent(MessageSubagent { subagent: subagent.into(), message })
            })
            .boxed(),
        CallKind::WaitForSubagents => (vec(name(), 0..3), of(any::<u64>()))
            .prop_map(|(subagents, timeout_ms)| {
                let subagents = Vec::from_iter(subagents.into_iter().map(Into::into));
                Call::WaitForSubagents(WaitForSubagents { subagents, timeout_ms })
            })
            .boxed(),
        CallKind::StopSubagent => {
            name().prop_map(|subagent| Call::StopSubagent(StopSubagent { subagent: subagent.into() })).boxed()
        }
        CallKind::ViewImage => name().prop_map(|path| Call::ViewImage(ViewImage { path })).boxed(),
        CallKind::GenerateImage => (text(), vec(name(), 0..2))
            .prop_map(|(prompt, from)| Call::GenerateImage(GenerateImage { prompt, from }))
            .boxed(),
        CallKind::Skill => (name(), of(text()))
            .prop_map(|(name, arguments)| Call::Skill(UseSkill { name: name.into(), arguments }))
            .boxed(),
        CallKind::Lsp => {
            use LspQuestion::*;
            let asks = select(
                [Definition, References, Implementation, Hover, Symbols, WorkspaceSymbols, Callers, Callees].as_slice(),
            );
            let at = of((any::<u32>(), any::<u32>()).prop_map(|(line, column)| Position { line, column }));
            (asks, name(), at).prop_map(|(asks, path, at)| Call::Lsp(Lsp { asks, path, at })).boxed()
        }
        CallKind::Advisor => Just(Call::Advisor).boxed(),
        CallKind::Sleep => any::<u64>().prop_map(|duration_ms| Call::Sleep(Sleep { duration_ms })).boxed(),
        CallKind::Monitor => (text(), of(text()), of(any::<u64>()))
            .prop_map(|(command, description, timeout_ms)| Call::Monitor(Monitor { command, description, timeout_ms }))
            .boxed(),
        CallKind::Schedule => {
            let when = prop_oneof![
                any::<u64>().prop_map(|seconds| When::After { seconds }),
                (text(), any::<bool>()).prop_map(|(expression, repeats)| When::Cron { expression, repeats }),
            ];
            (text(), when).prop_map(|(prompt, when)| Call::Schedule(Schedule { prompt, when })).boxed()
        }
        CallKind::Workflow => of(name()).prop_map(|name| Call::Workflow(Workflow { name })).boxed(),
        CallKind::Questions => Just(Call::Questions).boxed(),
        CallKind::Plan => Just(Call::Plan).boxed(),
        CallKind::Mcp => (name(), name(), value())
            .prop_map(|(server, tool, input)| Call::Mcp { server: server.into(), tool, input })
            .boxed(),
        CallKind::Other => (name(), value()).prop_map(|(tool, input)| Call::Other { tool, input }).boxed(),
        CallKind::Unknown => Just(Call::Unknown).boxed(),
    }
}

fn call() -> impl Strategy<Value = Call> {
    select(KINDS.as_slice()).prop_flat_map(call_of)
}

fn ended() -> impl Strategy<Value = Ended> {
    prop_oneof![
        select([Ended::Exited, Ended::TimedOut, Ended::Stopped].as_slice()),
        name().prop_map(|task| Ended::Backgrounded { task: task.into() }),
    ]
}

fn change() -> impl Strategy<Value = FileChange> {
    let hunk = (any::<u32>(), any::<u32>(), hunk_lines()).prop_map(|(old_start, new_start, lines)| Hunk {
        old_start,
        new_start,
        lines,
    });
    prop_oneof![Just(FileChange::Created), vec(hunk, 0..2).prop_map(|hunks| FileChange::Changed { hunks })]
}

fn paths() -> impl Strategy<Value = Paths> {
    (vec(name(), 0..3), any::<bool>()).prop_map(|(paths, truncated)| Paths { paths, truncated })
}

fn found() -> impl Strategy<Value = Found> {
    let line = (name(), any::<u32>(), text()).prop_map(|(path, line, text)| FoundLine { path, line, text });
    prop_oneof![
        (vec(line, 0..3), any::<bool>()).prop_map(|(lines, truncated)| Found::Lines { lines, truncated }),
        paths().prop_map(Found::Files),
        (any::<u64>(), any::<u64>()).prop_map(|(matches, files)| Found::Count { matches, files }),
    ]
}

fn outcome() -> impl Strategy<Value = FileOutcome> {
    let severity = select([Severity::Error, Severity::Warning, Severity::Information, Severity::Hint].as_slice());
    let diagnostic = (severity, any::<u32>(), of(any::<u32>()), text())
        .prop_map(|(severity, line, column, message)| Diagnostic { severity, line, column, message });
    (name(), of(change()), of(name()), vec(diagnostic, 0..2))
        .prop_map(|(path, change, moved_to, diagnostics)| FileOutcome { path, change, moved_to, diagnostics })
}

fn output() -> impl Strategy<Value = CallOutput> {
    let truncated = of(of(name()).prop_map(|full_at| Truncated { full_at }));
    let printed = (text(), of(any::<i32>()), ended(), of(any::<u64>()), truncated).prop_map(
        |(output, exit_code, ended, took_ms, truncated)| {
            CallOutput::Command(CommandOutput { output, exit_code, ended, took_ms, truncated })
        },
    );
    let result = (text(), text(), of(text())).prop_map(|(title, url, snippet)| WebResult { title, url, snippet });
    let content = prop_oneof![
        text().prop_map(|text| McpContent::Text { text }),
        image().prop_map(|image| McpContent::Image { image }),
        (name(), of(text())).prop_map(|(uri, text)| McpContent::Resource { uri, text }),
        value().prop_map(|value| McpContent::Other { value }),
        Just(McpContent::Unknown),
    ];
    // A `null` one is none, as the field's doc says.
    let structured = of(value().prop_filter("a structured result", |value| !value.is_null()));
    let read = prop_oneof![
        (any::<u32>(), vec(text(), 0..3), of(any::<u32>()))
            .prop_map(|(first, lines, total)| CallOutput::Lines(Lines { first, lines, total })),
        (image(), of(text())).prop_map(|(image, prompt)| CallOutput::Image { image, prompt }),
        of(any::<u32>()).prop_map(|pages| CallOutput::Document { pages }),
        vec(outcome(), 0..3).prop_map(|files| CallOutput::Changed { files }),
        found().prop_map(CallOutput::Found),
        paths().prop_map(CallOutput::Paths),
    ];
    let said = prop_oneof![
        (text(), of(any::<u16>()), text()).prop_map(|(url, status, text)| CallOutput::Page { url, status, text }),
        (vec(result, 0..3), of(text())).prop_map(|(results, summary)| CallOutput::WebResults { results, summary }),
        text().prop_map(|report| CallOutput::Report { report }),
        (vec(content, 0..3), structured).prop_map(|(content, structured)| CallOutput::Mcp { content, structured }),
        text().prop_map(|text| CallOutput::Words { text }),
        Just(CallOutput::Encrypted),
        of(name()).prop_map(|task| CallOutput::Backgrounded { task: task.map(Into::into) }),
    ];
    prop_oneof![printed, read, said]
}

fn denied() -> impl Strategy<Value = Option<Denied>> {
    of(of(text()).prop_map(|said| Denied { said }))
}

fn reads_back<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) -> Result<(), TestCaseError> {
    let written = serde_json::to_string(value).map_err(|e| TestCaseError::fail(e.to_string()))?;
    let read = serde_json::from_str::<T>(&written).map_err(|e| TestCaseError::fail(format!("{e}: {written}")))?;
    prop_assert_eq!(&read, value, "as {}", written);
    Ok(())
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn every_call_reads_back_as_it_was_written(id in name(), call in call()) {
        reads_back(&call)?;
        reads_back(&ContentBlock::Call { id: id.into(), call })?;
    }

    #[test]
    fn every_output_reads_back_as_it_was_written(
        id in name(),
        output in output(),
        is_error in any::<bool>(),
        denied in denied(),
        images in vec(image(), 0..2),
    ) {
        reads_back(&output)?;
        reads_back(&ContentBlock::CallResult { tool_use_id: id.into(), output, is_error, denied, images })?;
    }

    #[test]
    fn a_call_in_a_subagents_record_reads_back_as_it_was_written(
        id in name(),
        before in of((text(), any::<bool>()).prop_map(|(text, cut)| Said { text, cut })),
        call in call(),
        result in of((output(), any::<bool>(), any::<bool>(), vec(image(), 0..2), denied()).prop_map(
            |(output, is_error, cut, images, denied)| LatelyResult { output, is_error, cut, images, denied },
        )),
    ) {
        reads_back(&LatelyCall { before, id: id.into(), call, result })?;
    }
}
