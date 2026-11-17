//! What a subagent has done lately, with each call typed, as an adapter that types its calls records them: as
//! [`super::add`] keeps it, but each call as it was made, and what came back as its output, with each text it holds cut
//! short.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::{CommandOutput, FileOutcome, Found, FoundLine, Lines, McpContent, Paths};
use protocol::event::ContentBlock;
use protocol::model::{Lately, LatelyCall, LatelyResult};
use protocol::short_text::start_of;

use super::activity::{CALLS_KEPT, RESULT_KEPT, wrote_text};

/// Adds what one of its messages did, block by block, and says whether that changed anything, as [`super::add`] does
/// for the older form.
pub(in super::super) fn add(work: &mut Lately, blocks: &[ContentBlock], wrote: bool) -> bool {
    let mut changed = false;
    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } if wrote => changed |= wrote_text(&mut work.since, text),
            ContentBlock::Call { id, call } => {
                // Whole, as the record's newest call always has been, so its line says what it did.
                work.calls.push(LatelyCall {
                    before: work.since.take(),
                    id: id.clone(),
                    call: call.clone(),
                    result: None,
                });
                if work.calls.len() > CALLS_KEPT {
                    work.calls.remove(0);
                }
                changed = true;
            }
            ContentBlock::CallResult { tool_use_id, output, is_error, denied, images } => {
                let Some(made) = work.calls.iter_mut().find(|made| made.id == *tool_use_id) else { continue };
                let (output, cut) = kept(output);
                let (is_error, images, denied) = (*is_error, images.clone(), denied.clone());
                made.result = Some(LatelyResult { output, is_error, cut, images, denied });
                changed = true;
            }
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Unknown => {}
        }
    }
    changed
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
