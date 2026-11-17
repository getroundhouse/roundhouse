//! What came back for each kind of call, read from what the adapter kept of a result: its text, and beside it, how a
//! command exited and what an edit or a write changed. Where the text isn't what the kind sends back, it's kept as
//! words, as it came.

use protocol::capability::calls::call::{Edit, Write};
use protocol::capability::calls::output::{
    CommandOutput, Ended, FileOutcome, Found, FoundLine, Lines, McpContent, Paths, WebResult,
};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::FileChange;
use serde_json::Value;

/// How the CLI started the result of a command that exited with an error: this, then the code, on a line of its own.
const EXIT_CODE: &str = "Exit code ";
/// What the CLI put around a tool's own refusal.
const TOOL_ERROR_OPEN: &str = "<tool_use_error>";
const TOOL_ERROR_CLOSE: &str = "</tool_use_error>";
/// How the CLI started the result of a subagent's call that only said the subagent had started in the background, whose
/// report came later, as it ended.
const LAUNCHED: &str = "Async agent launched";

/// What came back for a call, as the adapter kept it.
pub(super) struct Came<'a> {
    pub text: &'a str,
    pub is_error: bool,
    pub exit_code: Option<i32>,
    pub encrypted: bool,
    pub change: Option<&'a FileChange>,
    pub answered: Answered,
}

/// What the adapter said of the call a result answered.
#[derive(Clone, Copy)]
pub(super) enum Answered {
    /// Nothing, as before it said, when the text was as the CLI sent it, with its line saying how a command exited,
    /// and its tags around a refusal. The adapter took both out once it said.
    Unsaid,
    /// A command, which matters only where the call isn't known.
    Command,
    /// Any other kind of call, or one it no longer knew.
    Other,
}

/// What came back for `call`, which is `None` where it isn't known, as when the CLI answered a call from before it
/// was started again. One that isn't known is read as a command where the adapter said it was one, or its text says
/// how a command exited, and as words otherwise.
pub(super) fn of(call: Option<&Call>, came: &Came<'_>) -> CallOutput {
    if came.encrypted {
        return CallOutput::Encrypted;
    }
    let commands = call.map_or(matches!(came.answered, Answered::Command), |call| matches!(call, Call::Command(_)));
    let as_sent = matches!(came.answered, Answered::Unsaid) && came.is_error;
    let (text, exit_code) = match exit_code(came.text).filter(|_| as_sent) {
        Some((code, printed)) if commands || call.is_none() => (printed.to_owned(), Some(code)),
        _ if as_sent && !commands => (refused(came.text), None),
        _ => (came.text.to_owned(), came.exit_code),
    };
    match call {
        _ if commands || exit_code.is_some() => command(text, exit_code),
        // What went wrong, in the tool's own words.
        _ if came.is_error => words(text),
        Some(Call::Read(_)) => lines(&text).unwrap_or_else(|| words(text)),
        Some(Call::Write(Write { path, .. }) | Call::Edit(Edit { path, .. })) => changed(path, came.change),
        Some(Call::Search(_) | Call::FindFiles(_)) => found(&text).unwrap_or_else(|| words(text)),
        Some(Call::Fetch(fetch)) => CallOutput::Page { url: fetch.url.clone(), status: None, text },
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

/// How a failed command exited, as the CLI said on its result's first line, and what it printed after that.
fn exit_code(text: &str) -> Option<(i32, &str)> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    Some((first.trim().strip_prefix(EXIT_CODE)?.parse().ok()?, rest))
}

/// A tool's own refusal, without the tags the CLI put around it.
fn refused(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.strip_prefix(TOOL_ERROR_OPEN) {
        Some(said) => said.trim_suffix(TOOL_ERROR_CLOSE).to_owned(),
        None => text.to_owned(),
    }
}

/// The lines a read sent back, each with its number in the file in front of it, then a tab, or an arrow in older
/// versions of the CLI. What follows the numbered lines, such as a note to the agent, isn't the file. `None` for a
/// read of something else, such as an image.
fn lines(text: &str) -> Option<CallOutput> {
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

/// What a write or an edit changed in the file at `path`, as the adapter kept it beside the result.
fn changed(path: &str, change: Option<&FileChange>) -> CallOutput {
    let file = FileOutcome { path: path.to_owned(), change: change.cloned(), moved_to: None, diagnostics: Vec::new() };
    CallOutput::Changed { files: vec![file] }
}

/// What a search found, as the CLI's words said it: its matching lines, as ripgrep prints them, files under a count of
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
    // `Grep` listed files under `Found 3 files`, and `Glob` listed their whole paths. Either may end with a note in
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

/// What a web search found, as the CLI's words said it: the pages, on lines of `Links: ` and their JSON, and what was
/// made of them after the last of those, without the first line, which said what was searched for, or the note the CLI
/// ended with, to cite the links, which isn't for a person.
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
