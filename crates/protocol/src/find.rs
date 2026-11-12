//! Finding text in a chat. What's looked for is a [`Needle`], which the hub and every client match the same way: the
//! hub to say which of a chat's events hold it, and a client to say where it is in what the chat shows of them.
//!
//! A chat shows Markdown as it renders it, so `**parser** is` shows as `parser is`, and a line break in a paragraph as
//! a space. The hub doesn't render it, so it looks past the characters Markdown marks text with, in [`MARKS`], and
//! past how much whitespace there is, and says an event holds `parser is` whether it's written plain, across bold,
//! italics, code or an escape, or across a line. A phrase across a link, which renders as its words alone, isn't found
//! that way.

use std::ops::Range;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::{ContentBlock, EventBody, FileChange};
use crate::model::Place;
use loose::{Loose, Written};

mod loose;
#[cfg(test)]
mod tests;

/// Some text to look for, matched ignoring case, as finding in a page does. A character matches one that's the same
/// once both are in lower case, as [`char::to_lowercase`] has them, so `É` finds `é`. A character whose lower case is
/// more than one, such as `İ`, matches only all of them, so a match always starts and ends between characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Needle {
    query: String,
    /// The query in lower case, which is what's compared.
    folded: Vec<char>,
    /// As looking past marks reads it.
    loose: Loose,
}

/// What Markdown marks text with that renders as nothing in the right places: emphasis, code, strikethrough and
/// escapes.
pub const MARKS: [char; 5] = ['*', '_', '`', '~', '\\'];

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NeedleError {
    #[error("there's nothing to look for")]
    Empty,
    #[error("that's longer than can be looked for")]
    TooLong,
}

/// Which of a chat's events may hold a needle, from [`crate::client::ClientRequest::SessionFind`], by their place among
/// the events of the agent or subagent it asked of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Every event that holds the needle somewhere in what it records, oldest first. It's more than a chat shows, so
    /// some of these show no match, and a client finds where each does in what it draws.
    pub places: Vec<Place>,
    /// The newest event looked at. Events after it hadn't been recorded, so a client looks for the needle in those
    /// itself, as the log brings them.
    pub through: Place,
}

impl Needle {
    /// The longest query looked for, in characters. Far longer than anything typed in a find bar, and short enough
    /// that a hub isn't asked to compare a pasted page against every event in a chat.
    pub const MOST_CHARS: usize = 1000;

    pub fn new(query: &str) -> Result<Self, NeedleError> {
        if query.is_empty() {
            return Err(NeedleError::Empty);
        }
        if query.chars().count() > Self::MOST_CHARS {
            return Err(NeedleError::TooLong);
        }
        let folded = query.chars().flat_map(char::to_lowercase).collect();
        Ok(Self { query: query.to_string(), folded, loose: Loose::of(query) })
    }

    /// What was asked for, as it was typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Where the needle is in `text`, left to right, in bytes. Matches don't overlap: each is looked for after the
    /// last, as a find bar steps through them.
    pub fn ranges<'a>(&'a self, text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        let ascii = text.is_ascii() && self.query.is_ascii();
        let mut from = 0;
        std::iter::from_fn(move || {
            let found = if ascii { self.next_ascii(text.as_bytes(), from) } else { self.next(text, from) }?;
            from = found.end;
            Some(found)
        })
    }

    pub fn is_in(&self, text: &str) -> bool {
        self.ranges(text).next().is_some()
    }

    /// Whether `text` may show the needle once it's rendered as Markdown: whether it holds the needle once [`MARKS`]
    /// are taken out of both, and each run of whitespace is one space. It holds whatever [`Needle::is_in`] finds too,
    /// so it's true of plain text that holds it. A needle that's nothing but marks is looked for as it is.
    pub fn may_be_in_markdown(&self, text: &str) -> bool {
        if self.loose.is_empty() {
            return self.is_in(text);
        }
        self.loose.is_in(text, Written::Plain)
    }

    /// Whether the event holds the needle in any text it records, as [`Needle::may_be_in_markdown`] looks: what was
    /// typed or written, every value, key and number in a call's input, also as the JSON a chat shows it as, what came
    /// back, the lines of a change, and what a subagent was for, was asked and reported. That's everything a chat shows
    /// of it, and more, so a client that finds where it shows can start from these.
    pub fn is_in_event(&self, body: &EventBody) -> bool {
        any_text(body, &mut |text| self.may_be_in_markdown(text))
    }

    /// Whether `json`, an event's body as `serde_json` writes it, may hold the needle anywhere [`Needle::is_in_event`]
    /// looks. It's false only when it certainly doesn't, so a hub can pass over most events without reading them.
    ///
    /// JSON writes a string as it is but for its escapes, which are read both ways: as they're written, as the JSON a
    /// call's input shows as has them, and as what they stand for, as text renders. JSON puts no space between its
    /// parts, where the JSON a call's input shows as has one after each colon and each part on a line of its own, so a
    /// needle with any of what JSON puts between its parts, or a space at either end, could be found there and not
    /// here, and isn't passed over.
    pub fn may_be_in_json(&self, json: &str) -> bool {
        let between = |c: char| matches!(c, '{' | '}' | '[' | ']' | ':' | ',') || c.is_control();
        if self.query.chars().any(between) || self.loose.has_space_at_an_end() {
            return true;
        }
        if self.loose.is_empty() {
            return self.is_in(json);
        }
        self.loose.is_in(json, Written::Plain) || self.loose.is_in(json, Written::Json)
    }

    /// The next match at or after `from`, when the text and the query are both ASCII, so a byte is a character.
    fn next_ascii(&self, text: &[u8], from: usize) -> Option<Range<usize>> {
        let query = self.query.as_bytes();
        let last = text.len().checked_sub(query.len())?;
        (from..=last).find(|&at| text[at..at + query.len()].eq_ignore_ascii_case(query)).map(|at| at..at + query.len())
    }

    /// The next match at or after `from`, which is between characters.
    fn next(&self, text: &str, from: usize) -> Option<Range<usize>> {
        text[from..].char_indices().find_map(|(at, _)| {
            let start = from + at;
            self.matches_at(&text[start..]).map(|len| start..start + len)
        })
    }

    /// How long a match at the start of `text` is, if there's one there.
    fn matches_at(&self, text: &str) -> Option<usize> {
        let mut wanted = self.folded.iter();
        for (at, c) in text.char_indices() {
            if !c.to_lowercase().all(|lower| wanted.next() == Some(&lower)) {
                return None;
            }
            if wanted.as_slice().is_empty() {
                return Some(at + c.len_utf8());
            }
        }
        None
    }
}

/// Whether `found` says yes for any text the event records, as [`Needle::is_in_event`] lists them.
fn any_text(body: &EventBody, found: &mut impl FnMut(&str) -> bool) -> bool {
    match body {
        EventBody::UserPrompt { prompt, .. } => found(&prompt.text),
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            blocks.iter().any(|block| any_text_in_block(block, found))
        }
        EventBody::ApprovalRequested { tool_name, input, description, .. } => {
            found(tool_name) || any_text_in_input(input, found) || description.as_deref().is_some_and(&mut *found)
        }
        EventBody::Notice { message, .. } => found(message),
        EventBody::SubagentStarted { kind, description, prompt, .. } => {
            [kind, description, prompt].into_iter().flatten().any(|text| found(text))
        }
        EventBody::SubagentEnded { report, .. } => report.as_deref().is_some_and(&mut *found),
        EventBody::AgentExited { stderr_tail, .. } => stderr_tail.iter().any(|line| found(line)),
        EventBody::TurnEnded { result, errors, .. } => {
            result.as_deref().is_some_and(&mut *found) || errors.iter().any(|error| found(error))
        }
        EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        // A compaction isn't found in, as `docs/spec/chats.md` says.
        | EventBody::CompactionAsked { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        // Nor is a cache that expired, which says only how long and how much.
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => false,
    }
}

fn any_text_in_block(block: &ContentBlock, found: &mut impl FnMut(&str) -> bool) -> bool {
    match block {
        ContentBlock::Text { text, .. } | ContentBlock::Thinking { text } => found(text),
        ContentBlock::ToolUse { input, .. } => any_text_in_input(input, found),
        ContentBlock::ToolResult { text, change, .. } => {
            let lines = match change {
                Some(FileChange::Changed { hunks }) => hunks.iter().flat_map(|hunk| &hunk.lines).collect(),
                Some(FileChange::Created | FileChange::Unknown) | None => Vec::new(),
            };
            found(text) || lines.iter().any(|line| found(&line.text))
        }
        ContentBlock::Image { .. } | ContentBlock::Unknown => false,
    }
}

/// A call's input: each of its strings as it is, and all of it as the JSON a chat shows it as, which has its keys and
/// numbers too.
fn any_text_in_input(input: &Value, found: &mut impl FnMut(&str) -> bool) -> bool {
    fn strings(value: &Value, found: &mut impl FnMut(&str) -> bool) -> bool {
        match value {
            Value::String(text) => found(text),
            Value::Array(values) => values.iter().any(|value| strings(value, found)),
            Value::Object(fields) => fields.values().any(|value| strings(value, found)),
            Value::Null | Value::Bool(_) | Value::Number(_) => false,
        }
    }
    strings(input, found) || serde_json::to_string_pretty(input).is_ok_and(|json| found(&json))
}
