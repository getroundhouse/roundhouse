//! Extracting domain terms, file stems and code identifiers in order of how close they are to what the person is
//! writing, budgeting conversation messages apart from tool calls so a tool-heavy turn doesn't push recent chat out.

use std::collections::HashSet;

use protocol::event::{ContentBlock, EventBody, Question};

use super::english::{all_common_english, is_boilerplate};
use crate::calls::{EditFile, ReadFile, Recorded, ToSubagents, ToolCall, WriteFile};
use crate::records::InWorkspace;
use crate::session_log::{Row, SessionLog};

const MAX_HINTS: usize = 160;
const MAX_TEXT_MESSAGES: usize = 24;
const MAX_TOOL_EVENTS: usize = 30;
const MAX_PATH_PARENTS: usize = 2;

/// Accumulates candidate hint phrases, most immediate first, deduplicating case-insensitively on [`Self::finish`].
#[derive(Default)]
pub struct HintCollector {
    seen: HashSet<String>,
    out: Vec<String>,
}

impl HintCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pre-vetted name such as a repository name.
    pub fn add_name(&mut self, raw: &str) {
        self.insert(raw, true);
    }

    pub(super) fn add_term(&mut self, raw: &str) {
        self.insert(raw, false);
    }

    pub(super) fn add_proper_noun(&mut self, raw: &str) {
        self.insert(raw, true);
    }

    fn insert(&mut self, raw: &str, proper_noun: bool) {
        if self.out.len() >= MAX_HINTS {
            return;
        }
        let cleaned = raw.trim().trim_matches(|ch: char| {
            matches!(ch, '`' | '"' | '\'' | ',' | ';' | ':' | '.' | '(' | ')' | '[' | ']' | '{' | '}' | '*')
        });
        if !worth_keeping(cleaned) || (!proper_noun && all_common_english(cleaned)) {
            return;
        }
        if self.seen.insert(cleaned.to_lowercase()) {
            self.out.push(cleaned.to_string());
        }
    }

    /// Adds a question's header, short option labels, and code spans or identifiers in its text and descriptions.
    pub fn add_question(&mut self, question: &Question) {
        if let Some(header) = &question.header {
            self.add_short_label(header);
        }
        for option in &question.options {
            self.add_short_label(&option.label);
            self.add_prose(&option.label);
            if let Some(desc) = &option.description {
                self.add_prose(desc);
            }
        }
        self.add_prose(&question.text);
    }

    /// Adds the leaf file stem and up to [`MAX_PATH_PARENTS`] meaningful parent segments of `path`, leaf first.
    pub fn add_path(&mut self, path: &str) {
        let mut segments: Vec<&str> =
            path.split(['/', '\\']).map(str::trim).filter(|seg| !seg.is_empty() && !seg.starts_with('.')).collect();
        if let Some(leaf) = segments.pop() {
            let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem);
            if !is_boilerplate(stem) {
                self.add_term(stem);
            }
        }
        let mut added_parents = 0;
        for seg in segments.into_iter().rev() {
            if added_parents >= MAX_PATH_PARENTS {
                break;
            }
            if !is_boilerplate(seg) {
                self.add_term(seg);
                added_parents += 1;
            }
        }
    }

    /// Adds the workspace's name, branches, repository name, pull request title terms and commented diff paths.
    pub fn add_workspace(&mut self, shown: &InWorkspace) {
        let workspace = shown.workspace();
        self.add_proper_noun(&workspace.name);
        self.add_branch(workspace.branch.as_str());
        if let Some(branches) = &workspace.branches {
            if let Some(checked_out) = branches.checked_out() {
                self.add_branch(checked_out.name.as_str());
            }
            for branch in &branches.had {
                self.add_branch(branch.name.as_str());
            }
        }
        if let Some(repo) = shown.all().repo(&workspace.repo_id) {
            self.add_proper_noun(&repo.name);
        }
        if let Some(pr) = workspace.known_pull_request() {
            self.add_prose(&pr.title);
        }
        for comment in shown.comments().iter().rev() {
            self.add_path(&comment.path);
        }
    }

    /// Walks `log` newest-first with separate budgets for conversation messages and tool calls, so a turn with dozens
    /// of tool calls doesn't crowd out the names discussed a turn or two earlier.
    pub fn add_session_log(&mut self, log: &SessionLog) {
        let mut text_messages = 0;
        let mut tool_events = 0;
        for index in (0..log.row_count()).rev() {
            if (text_messages >= MAX_TEXT_MESSAGES && tool_events >= MAX_TOOL_EVENTS) || self.out.len() >= MAX_HINTS {
                break;
            }
            let Row::Event(event) = log.row(index) else { continue };
            self.add_event(&event.body, &mut text_messages, &mut tool_events);
        }
    }

    pub fn finish(self) -> Vec<String> {
        self.out
    }

    fn add_short_label(&mut self, label: &str) {
        let clean = label.strip_prefix("(Recommended)").map_or(label, str::trim).trim();
        if clean.split_whitespace().count() <= 3 && !all_common_english(clean) {
            self.add_proper_noun(clean);
        }
    }

    fn add_branch(&mut self, branch: &str) {
        for seg in branch.split('/') {
            if !is_boilerplate(seg) {
                self.add_term(seg);
            }
        }
    }

    fn add_event(&mut self, body: &EventBody, text_messages: &mut usize, tool_events: &mut usize) {
        match body {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
                self.add_blocks(blocks, text_messages, tool_events);
            }
            EventBody::UserPrompt { prompt, .. } if *text_messages < MAX_TEXT_MESSAGES => {
                for file in &prompt.files {
                    self.add_path(&file.name);
                }
                self.add_prose(&prompt.text);
                *text_messages += 1;
            }
            EventBody::ApprovalRequested { .. } if *tool_events < MAX_TOOL_EVENTS => {
                if let Some(call) = Recorded::asked(body) {
                    self.add_call(ToolCall::read(call));
                    *tool_events += 1;
                }
            }
            EventBody::SubagentStarted { description, prompt, .. } if *tool_events < MAX_TOOL_EVENTS => {
                if let Some(desc) = description {
                    self.add_prose(desc);
                }
                if let Some(p) = prompt {
                    self.add_prose(p);
                }
                *tool_events += 1;
            }
            EventBody::Notice { message, .. } if *text_messages < MAX_TEXT_MESSAGES => {
                self.add_prose(message);
                *text_messages += 1;
            }
            EventBody::UserPrompt { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::SubagentStarted { .. }
            | EventBody::Notice { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::OwnTurn(_)
            | EventBody::AgentLimits { .. }
            | EventBody::SubagentContext { .. }
            | EventBody::SubagentEnded { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::TurnEnded { .. }
            | EventBody::RequestFailed { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::AgentExited { .. }
            | EventBody::AgentMode { .. }
            | EventBody::TaskChanged { .. }
            | EventBody::AgentOther { .. }
            | EventBody::AgentCompacting { .. }
            | EventBody::CompactionAsked { .. }
            | EventBody::AgentCompacted { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::Unknown => {}
        }
    }

    fn add_blocks(&mut self, blocks: &[ContentBlock], text_messages: &mut usize, tool_events: &mut usize) {
        let mut had_text = false;
        let mut had_tool = false;
        for block in blocks.iter().rev() {
            if let Some(recorded) = Recorded::of(block) {
                if *tool_events < MAX_TOOL_EVENTS {
                    self.add_call(ToolCall::read(recorded));
                    had_tool = true;
                }
            } else if let ContentBlock::Text { text, .. } = block
                && *text_messages < MAX_TEXT_MESSAGES
            {
                self.add_prose(text);
                had_text = true;
            }
        }
        *text_messages += usize::from(had_text);
        *tool_events += usize::from(had_tool);
    }

    fn add_call(&mut self, call: ToolCall<'_>) {
        match call {
            ToolCall::Read(ReadFile { path })
            | ToolCall::Write(WriteFile { path, .. })
            | ToolCall::Edit(EditFile { path, .. }) => self.add_path(path),
            ToolCall::NotebookEdit(edit) => self.add_path(&edit.path),
            ToolCall::Patch(patch) => {
                for file in &patch.files {
                    self.add_path(&file.path);
                }
            }
            ToolCall::Search(search) => {
                if let Some(path) = search.path {
                    self.add_path(path);
                }
                self.add_prose(search.pattern);
            }
            ToolCall::Command(command) => self.add_prose(command.summary()),
            ToolCall::Agent(agent) => {
                if let Some(desc) = agent.described() {
                    self.add_prose(desc);
                }
            }
            ToolCall::ToSubagents(ToSubagents::Message { message, .. }) => self.add_prose(message),
            ToolCall::Typed(_) | ToolCall::Other(_) => {
                if let Some(summary) = call.summary() {
                    self.add_prose(&summary);
                }
            }
            // A wait or a stop names its subagents by their IDs, which aren't anyone's words.
            ToolCall::ToSubagents(ToSubagents::Wait { .. } | ToSubagents::Stop { .. })
            | ToolCall::Fetch(_)
            | ToolCall::WebSearch(_)
            | ToolCall::Advisor => {}
        }
    }
}

fn worth_keeping(cleaned: &str) -> bool {
    if cleaned.len() < 2 || cleaned.len() > 48 || cleaned.starts_with('-') || is_boilerplate(cleaned) {
        return false;
    }
    if cleaned.len() == 2 && !cleaned.chars().all(|ch| ch.is_ascii_uppercase()) {
        return false;
    }
    if !cleaned.chars().any(char::is_alphabetic) {
        return false;
    }
    if !cleaned.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | '\'')) {
        return false;
    }
    if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let hex_or_dash = cleaned.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let hex_digits = cleaned.chars().filter(char::is_ascii_hexdigit).count();
    !(hex_or_dash && hex_digits >= 8)
}
