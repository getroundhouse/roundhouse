//! What a subagent has done lately, from its events, which its call shows while it works: its newest calls, each as it
//! was made, with the text it wrote just before it and what came back, then what it's written since. Every change to a
//! subagent's record goes to every client, so only a few calls are kept, and each text is cut short. A call is kept
//! whole, so its line says what it did, such as how many lines a write wrote. A write holds the whole file, so that's
//! what can still make a record large. The hub keeps a subagent's record by this, and a fake hub makes one by it.

use super::record::{Lately, LatelyCall, LatelyResult, Said};
use crate::capability::calls::CallOutput;
use crate::capability::calls::output::{CommandOutput, FileOutcome, Found, FoundLine, Lines, McpContent, Paths};
use crate::event::ContentBlock;
use crate::short_text::start_of;

/// How many of its newest calls are kept, which is as many as its call shows.
pub(super) const CALLS_KEPT: usize = 3;
/// How much of what it writes between calls is kept, in characters: a long paragraph, more than its call shows of it.
pub(super) const TEXT_KEPT: usize = 1_000;
/// How much of what came back for a call is kept, in characters: all of a short one, such as most commands' and small
/// reads', which its call's line then counts, and the start of a long one, which says what went wrong if it failed. A
/// whole file read can be hundreds of kilobytes, and would go to every client with each of the subagent's next few
/// changes.
pub(super) const RESULT_KEPT: usize = 2_000;

impl Lately {
    /// Adds what one of its messages did, block by block, and says whether that changed anything. `wrote` says the
    /// message is the subagent's own, so its text is what it wrote. Text in a `user` line was written to it, such as
    /// the prompt the CLI sends a subagent run in the foreground, which its call already shows, so only what came back
    /// is taken from one.
    pub fn add(&mut self, blocks: &[ContentBlock], wrote: bool) -> bool {
        let mut changed = false;
        for block in blocks {
            match block {
                ContentBlock::Text { text, .. } if wrote => changed |= wrote_text(&mut self.since, text),
                ContentBlock::Call { id, call } => {
                    // Whole, as the record's newest call always has been, so its line says what it did.
                    self.calls.push(LatelyCall {
                        before: self.since.take(),
                        id: id.clone(),
                        call: call.clone(),
                        result: None,
                    });
                    if self.calls.len() > CALLS_KEPT {
                        self.calls.remove(0);
                    }
                    changed = true;
                }
                ContentBlock::CallResult { tool_use_id, output, is_error, denied, images } => {
                    let Some(made) = self.calls.iter_mut().find(|made| made.id == *tool_use_id) else { continue };
                    let (output, cut) = kept(output);
                    let (is_error, images, denied) = (*is_error, images.clone(), denied.clone());
                    made.result = Some(LatelyResult { output, is_error, cut, images, denied });
                    changed = true;
                }
                ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::Unknown => {}
            }
        }
        changed
    }
}

/// What came back, as a record keeps it, and whether any of it was cut: each text it holds, in the order they come,
/// kept as far as [`RESULT_KEPT`] characters go between them, and what it changed in a file left off, since that can
/// be as long as the file. Everything else is as it came.
pub(super) fn kept(output: &CallOutput) -> (CallOutput, bool) {
    let mut left = Left { characters: RESULT_KEPT, cut: false };
    let kept = match output {
        CallOutput::Command(command) => {
            CallOutput::Command(CommandOutput { output: left.text(&command.output), ..command.clone() })
        }
        CallOutput::Lines(read) => CallOutput::Lines(Lines { lines: left.texts(&read.lines), ..read.clone() }),
        CallOutput::Changed { files } => CallOutput::Changed {
            files: files.iter().map(|file| FileOutcome { change: None, ..file.clone() }).collect(),
        },
        CallOutput::Found(Found::Lines { lines, truncated }) => {
            let lines = lines.iter().map_while(|line| left.found(line)).collect();
            CallOutput::Found(Found::Lines { lines, truncated: *truncated })
        }
        CallOutput::Found(Found::Files(files)) => CallOutput::Found(Found::Files(left.paths(files))),
        CallOutput::Paths(paths) => CallOutput::Paths(left.paths(paths)),
        CallOutput::Page { url, status, text } => {
            CallOutput::Page { url: url.clone(), status: *status, text: left.text(text) }
        }
        CallOutput::WebResults { results, summary } => {
            CallOutput::WebResults { results: results.clone(), summary: summary.as_deref().map(|said| left.text(said)) }
        }
        CallOutput::Report { report } => CallOutput::Report { report: left.text(report) },
        CallOutput::Mcp { content, structured } => {
            let content = content
                .iter()
                .map(|block| match block {
                    McpContent::Text { text } => McpContent::Text { text: left.text(text) },
                    McpContent::Image { .. }
                    | McpContent::Resource { .. }
                    | McpContent::Other { .. }
                    | McpContent::Unknown => block.clone(),
                })
                .collect();
            CallOutput::Mcp { content, structured: structured.clone() }
        }
        CallOutput::Words { text } => CallOutput::Words { text: left.text(text) },
        CallOutput::Image { .. }
        | CallOutput::Document { .. }
        | CallOutput::Found(Found::Count { .. } | Found::Unknown)
        | CallOutput::Encrypted
        | CallOutput::Backgrounded { .. }
        | CallOutput::Unknown => output.clone(),
    };
    (kept, left.cut)
}

/// How many characters of what came back are still to be kept, and whether any have been cut.
struct Left {
    characters: usize,
    cut: bool,
}

impl Left {
    /// As much of `text` as is left to keep.
    fn text(&mut self, text: &str) -> String {
        let (kept, cut) = start_of(text, self.characters);
        self.characters -= kept.chars().count();
        self.cut |= cut;
        kept.to_owned()
    }

    /// Each of `texts` as far as what's left goes, and none once nothing is.
    fn texts(&mut self, texts: &[String]) -> Vec<String> {
        texts.iter().map_while(|text| self.any_left(text).then(|| self.text(text))).collect()
    }

    /// A line a search found, if anything's left to keep of it.
    fn found(&mut self, line: &FoundLine) -> Option<FoundLine> {
        self.any_left(&line.text).then(|| FoundLine { text: self.text(&line.text), ..line.clone() })
    }

    fn paths(&mut self, paths: &Paths) -> Paths {
        Paths { paths: self.texts(&paths.paths), truncated: paths.truncated }
    }

    /// Whether anything's left to keep of `text`, saying it was cut where nothing is.
    fn any_left(&mut self, text: &str) -> bool {
        if self.characters == 0 && !text.is_empty() {
            self.cut = true;
            return false;
        }
        true
    }
}

/// Adds `text`, which it wrote, to what it's written `since` its newest call, and says whether that changed it.
fn wrote_text(since: &mut Option<Said>, text: &str) -> bool {
    // Once what it's written since is cut short, more of it changes nothing, so it sends no notice.
    if text.trim().is_empty() || since.as_ref().is_some_and(|since| since.cut) {
        return false;
    }
    let joined = match since.take() {
        Some(since) => said(&format!("{}\n\n{text}", since.text)),
        None => said(text),
    };
    *since = Some(joined);
    true
}

fn said(text: &str) -> Said {
    let (text, cut) = start_of(text, TEXT_KEPT);
    Said { text: text.to_owned(), cut }
}
