//! What came back for each kind of call, read from the CLI's words for it, and from what it says beside them in the
//! line's `tool_use_result`, so that no client reads the CLI's words. Where the words aren't what the kind sends back,
//! they're kept as words, as they came.

use protocol::capability::calls::call::Read;
use protocol::capability::calls::output::{
    CommandOutput, Ended, FileOutcome, Found, FoundLine, Lines, McpContent, Paths, WebResult,
};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::ToolUseId;
use serde_json::Value;

use super::super::normalize::file_change;

/// How the CLI starts the result of a command that exited with an error: this, then the code, on a line of its own.
const EXIT_CODE: &str = "Exit code ";
/// What the CLI puts around a tool's own refusal.
const TOOL_ERROR_OPEN: &str = "<tool_use_error>";
const TOOL_ERROR_CLOSE: &str = "</tool_use_error>";
/// How the CLI starts the result of a subagent's call that only says the subagent started in the background, whose
/// report comes as it ends.
const LAUNCHED: &str = "Async agent launched";

/// Says what came back for each result in `body`, which come as the CLI's words, as what the call it answers sends
/// back, from those words and from what `line`, the line they were read from, says beside them. `answered` is each
/// call the line answers, by its ID.
pub(in super::super) fn say(body: &mut EventBody, answered: &[(ToolUseId, Call)], line: &Value) {
    let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else { return };
    for block in blocks {
        let ContentBlock::CallResult { tool_use_id, output, is_error, .. } = block else { continue };
        let CallOutput::Words { text } = output else { continue };
        let text = std::mem::take(text);
        let call = answered.iter().find(|(id, _)| id == tool_use_id).map(|(_, call)| call);
        *output = of(call, text, *is_error, line);
    }
}

/// What came back for `call`, in the CLI's words `text`, on the line `line`. A call this adapter doesn't know, as when
/// the CLI answers one from before it was started again, is read as a command's would be when it failed, since the CLI
/// says how a command exited whatever this adapter knows of the call, and as words otherwise.
pub(super) fn of(call: Option<&Call>, text: String, is_error: bool, line: &Value) -> CallOutput {
    let result = line.get("tool_use_result");
    match call {
        Some(Call::Command(_)) | None if is_error => match exit_code(&text) {
            Some((code, printed)) => command(printed.to_owned(), Some(code)),
            None if call.is_none() => words(refused(&text)),
            None => command(text, None),
        },
        // What went wrong, in the tool's own words.
        Some(_) if is_error => words(refused(&text)),
        // The CLI names the file a change it says beside a result is to, so it's kept whatever the call was.
        None if let Some(path) = result.and_then(|result| result.get("filePath")?.as_str())
            && file_change::read(line).is_some() =>
        {
            changed(path, line)
        }
        Some(Call::Command(_)) => command(text, None),
        Some(Call::Read(read)) => lines(read, &text, result).unwrap_or_else(|| words(text)),
        Some(Call::Write(write)) => changed(&write.path, line),
        Some(Call::Edit(edit)) => changed(&edit.path, line),
        Some(Call::Search(_) | Call::FindFiles(_)) => found(&text).unwrap_or_else(|| words(text)),
        Some(Call::Fetch(fetch)) => CallOutput::Page {
            url: fetch.url.clone(),
            status: result.and_then(|result| result.get("code")?.as_u64()?.try_into().ok()),
            text,
        },
        Some(Call::WebSearch(_)) => web_results(&text),
        Some(Call::Subagent(_)) if text.starts_with(LAUNCHED) => CallOutput::Backgrounded { task: None },
        Some(Call::Subagent(_)) => CallOutput::Report { report: text },
        Some(Call::Mcp { .. }) => {
            let content = if text.is_empty() { Vec::new() } else { vec![McpContent::Text { text }] };
            CallOutput::Mcp { content, structured: None }
        }
        Some(_) | None => words(text),
    }
}

fn words(text: String) -> CallOutput {
    CallOutput::Words { text }
}

fn command(output: String, exit_code: Option<i32>) -> CallOutput {
    CallOutput::Command(CommandOutput { output, exit_code, ended: Ended::Exited, took_ms: None, truncated: None })
}

/// How a failed command exited, as the CLI says on its result's first line, and what it printed after that.
fn exit_code(text: &str) -> Option<(i32, &str)> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    Some((first.trim().strip_prefix(EXIT_CODE)?.parse().ok()?, rest))
}

/// A tool's own refusal, without the tags the CLI puts around it.
fn refused(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.strip_prefix(TOOL_ERROR_OPEN) {
        Some(said) => said.trim_suffix(TOOL_ERROR_CLOSE).to_owned(),
        None => text.to_owned(),
    }
}

/// The lines a read sent back, each with its number in the file: as the CLI says them beside its words, or as its words
/// say them, each line's number in front of it, then a tab, or an arrow in older versions. What follows the numbered
/// lines, such as a note to the agent, isn't the file. `None` for a read of something else, such as an image.
fn lines(read: &Read, text: &str, result: Option<&Value>) -> Option<CallOutput> {
    let file = result
        .filter(|result| result.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|result| result.get("file"));
    if let Some(file) = file
        && let Some(content) = file.get("content").and_then(Value::as_str)
    {
        let number = |key| file.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
        let first = number("startLine").or_else(|| read.lines.as_ref().map(|lines| lines.first)).unwrap_or(1);
        return Some(CallOutput::Lines(Lines {
            first,
            lines: content.lines().map(str::to_owned).collect(),
            total: number("totalLines"),
        }));
    }
    let numbered: Vec<(u32, &str)> = text.lines().map_while(numbered).collect();
    let first = numbered.first()?.0;
    Some(CallOutput::Lines(Lines {
        first,
        lines: numbered.into_iter().map(|(_, line)| line.to_owned()).collect(),
        total: None,
    }))
}

fn numbered(line: &str) -> Option<(u32, &str)> {
    let line = line.trim_start_matches(' ');
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let number = line[..digits].parse().ok()?;
    let rest = &line[digits..];
    Some((number, rest.strip_prefix('\t').or_else(|| rest.strip_prefix('→'))?))
}

/// What a write or an edit changed in the file at `path`, as the CLI says it beside its words. A change too long to
/// carry is left to the call.
fn changed(path: &str, line: &Value) -> CallOutput {
    let file =
        FileOutcome { path: path.to_owned(), change: file_change::read(line), moved_to: None, diagnostics: Vec::new() };
    CallOutput::Changed { files: vec![file] }
}

/// What a search found, as the CLI's words say it: its matching lines, as ripgrep prints them, files under a count of
/// them or as their whole paths, or that it found none. `None` for words that say anything else, such as lines around
/// each match.
fn found(text: &str) -> Option<CallOutput> {
    let said: Vec<&str> = text.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect();
    if said.iter().all(|line| line.starts_with("No ") && line.ends_with(" found")) {
        return Some(CallOutput::Found(Found::Files(Paths { paths: Vec::new(), truncated: false })));
    }
    if let Some(lines) = said.iter().map(|line| matched_line(line)).collect::<Option<Vec<_>>>() {
        return Some(CallOutput::Found(Found::Lines { lines, truncated: false }));
    }
    // `Grep` lists files under `Found 3 files`, and `Glob` lists their whole paths. Either may end with a note in
    // brackets, such as that the list was cut short.
    let (notes, listed): (Vec<&str>, Vec<&str>) = said.into_iter().partition(|line| line.starts_with('('));
    let paths = match listed.split_first() {
        Some((first, files)) if is_count_of_files(first) => files.to_vec(),
        Some(_) if listed.iter().all(|line| line.starts_with('/')) => listed,
        _ => return None,
    };
    let paths = paths.into_iter().map(str::to_owned).collect();
    Some(CallOutput::Found(Found::Files(Paths { paths, truncated: !notes.is_empty() })))
}

/// `path:number:text`, as ripgrep prints a match.
fn matched_line(line: &str) -> Option<FoundLine> {
    let (path, rest) = line.split_once(':')?;
    let (number, text) = rest.split_once(':')?;
    Some(FoundLine { path: path.to_owned(), line: number.parse().ok()?, text: text.to_owned() })
}

/// `Found 3 files`, or `Found 1 file`.
fn is_count_of_files(line: &str) -> bool {
    let words: Vec<&str> = line.split(' ').collect();
    matches!(words.as_slice(), ["Found", n, "file" | "files"] if n.parse::<u32>().is_ok())
}

/// What a web search found, as the CLI's words say it: the pages, on lines of `Links: ` and their JSON, and what was
/// made of them after the last of those, without the first line, which says what was searched for, or the note the CLI
/// ends with, to cite the links, which isn't for a person.
fn web_results(text: &str) -> CallOutput {
    let (mut results, mut after_links, mut at) = (Vec::new(), None, 0);
    for line in text.split_inclusive('\n') {
        at += line.len();
        if let Some(found) = line.trim_end().strip_prefix("Links: ").and_then(links) {
            results.extend(found);
            after_links = Some(at);
        }
    }
    let summary = match after_links {
        Some(at) => &text[at..],
        None if text.starts_with("Web search results") => text.split_once('\n').map_or("", |(_, rest)| rest),
        None => text,
    };
    let summary = summary.rfind("\nREMINDER:").map_or(summary, |at| &summary[..at]).trim();
    CallOutput::WebResults { results, summary: Some(summary.to_owned()).filter(|summary| !summary.is_empty()) }
}

fn links(json: &str) -> Option<Vec<WebResult>> {
    let links: Vec<Value> = serde_json::from_str(json).ok()?;
    let link = |link: &Value| {
        let url = link.get("url")?.as_str()?.to_owned();
        let title = link.get("title").and_then(Value::as_str).map_or_else(|| url.clone(), str::to_owned);
        Some(WebResult { title, url, snippet: None })
    };
    Some(links.iter().filter_map(link).collect())
}
