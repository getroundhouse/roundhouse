//! Extracting domain terms, file stems and code identifiers in order of how close they are to what the person is
//! writing, budgeting conversation messages apart from tool calls so a tool-heavy turn doesn't push recent chat out.

use std::collections::HashMap;

use hint_words::{Asked, HintWord, PickedFrom, Proposal, Workspace};
use protocol::event::{ContentBlock, EventBody};

use super::english::{all_common_english, is_boilerplate};
use crate::calls::{EditFile, ReadFile, Recorded, ToSubagents, ToolCall, WriteFile};
use crate::session_log::{Row, SessionLog};

const MAX_HINTS: usize = 160;
const MAX_TEXT_MESSAGES: usize = 24;
const MAX_TOOL_EVENTS: usize = 30;
const MAX_PATH_PARENTS: usize = 2;
/// A proposal's Markdown can be long, so it gives at most this many, leaving room for the names around it.
const MAX_PROPOSAL_MARKDOWN_HINTS: usize = 40;

/// The words to favour, most immediate first: what's typed, what the field is about, the workspace's, then those of
/// `chat`, which `from` says what was loaded of. What's kept of a dictation is `from`, so nothing else goes into them.
pub fn pick(from: &PickedFrom, chat: Option<&SessionLog>) -> Vec<HintWord> {
    let mut collector = HintCollector::default();
    collector.add_prose(&from.typed);
    collector.add_paths(from.attached.iter().chain(&from.open_file));
    if let Some(question) = &from.question {
        collector.add_asked(question);
    }
    if let Some(proposal) = &from.proposal {
        collector.add_proposal(proposal);
    }
    collector.add_paths(&from.comment_on);
    for name in &from.repos {
        collector.add_own_name(name);
    }
    if let Some(workspace) = &from.workspace {
        collector.add_workspace(workspace);
    }
    collector.add_paths(&from.changed);
    if let Some(chat) = chat {
        collector.add_session_log(chat);
    }
    collector.out
}

/// Accumulates candidate hint phrases, most immediate first, deduplicating case-insensitively, each written as it's
/// hinted unless only the workspace's own names gave it.
pub(super) struct HintCollector {
    /// Where each phrase is in `out`, by its lower case.
    seen: HashMap<String, usize>,
    out: Vec<HintWord>,
    /// How many it holds before it takes no more, which is [`MAX_HINTS`] but while one source is held to a budget.
    limit: usize,
}

impl Default for HintCollector {
    fn default() -> Self {
        Self { seen: HashMap::new(), out: Vec::new(), limit: MAX_HINTS }
    }
}

impl HintCollector {
    /// Adds one of the workspace's own names, already vetted, such as a repository's.
    fn add_own_name(&mut self, raw: &str) {
        self.insert(raw, true, Whose::Own);
    }

    pub(super) fn add_term(&mut self, raw: &str) {
        self.insert(raw, false, Whose::Named);
    }

    pub(super) fn add_proper_noun(&mut self, raw: &str) {
        self.insert(raw, true, Whose::Named);
    }

    /// Adds a word of one of the workspace's own names, such as a branch's or a file's.
    fn add_own_term(&mut self, raw: &str) {
        self.insert(raw, false, Whose::Own);
    }

    fn insert(&mut self, raw: &str, proper_noun: bool, whose: Whose) {
        if self.out.len() >= self.limit {
            return;
        }
        let cleaned = raw.trim().trim_matches(|ch: char| {
            matches!(ch, '`' | '"' | '\'' | ',' | ';' | ':' | '.' | '(' | ')' | '[' | ']' | '{' | '}' | '*')
        });
        if !worth_keeping(cleaned) || (!proper_noun && all_common_english(cleaned)) {
            return;
        }
        let spelled = whose == Whose::Named;
        if let Some(&at) = self.seen.get(&cleaned.to_lowercase()) {
            self.out[at].spelled |= spelled;
        } else {
            self.seen.insert(cleaned.to_lowercase(), self.out.len());
            self.out.push(HintWord { text: cleaned.to_string(), spelled });
        }
    }

    /// Adds a proposal's title, then each of its questions as [`Self::add_asked`] does, then code spans or identifiers
    /// in its Markdown, which is longer and less likely said, up to [`MAX_PROPOSAL_MARKDOWN_HINTS`].
    fn add_proposal(&mut self, proposal: &Proposal) {
        self.add_prose(&proposal.title);
        for question in &proposal.questions {
            self.add_asked(question);
        }
        let limit = std::mem::replace(&mut self.limit, (self.out.len() + MAX_PROPOSAL_MARKDOWN_HINTS).min(MAX_HINTS));
        for markdown in &proposal.markdown {
            self.add_prose(markdown);
        }
        self.limit = limit;
    }

    /// Adds a question's header, short option labels, and code spans or identifiers in its options, then in its text.
    fn add_asked(&mut self, asked: &Asked) {
        if let Some(header) = &asked.header {
            self.add_short_label(header);
        }
        for option in &asked.options {
            self.add_short_label(&option.label);
            self.add_prose(&option.label);
            if let Some(description) = &option.description {
                self.add_prose(description);
            }
        }
        self.add_prose(&asked.text);
    }

    fn add_paths<'a>(&mut self, paths: impl IntoIterator<Item = &'a String>) {
        for path in paths {
            self.add_path(path);
        }
    }

    /// Adds the leaf file stem and up to [`MAX_PATH_PARENTS`] meaningful parent segments of `path`, leaf first.
    pub(super) fn add_path(&mut self, path: &str) {
        let mut segments: Vec<&str> =
            path.split(['/', '\\']).map(str::trim).filter(|seg| !seg.is_empty() && !seg.starts_with('.')).collect();
        if let Some(leaf) = segments.pop() {
            let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem);
            if !is_boilerplate(stem) {
                self.add_own_term(stem);
            }
        }
        let mut added_parents = 0;
        for seg in segments.into_iter().rev() {
            if added_parents >= MAX_PATH_PARENTS {
                break;
            }
            if !is_boilerplate(seg) {
                self.add_own_term(seg);
                added_parents += 1;
            }
        }
    }

    /// Adds the workspace's name, branches, repository name, commented diff paths and pull request's title.
    fn add_workspace(&mut self, workspace: &Workspace) {
        self.add_own_name(&workspace.name);
        for branch in &workspace.branches {
            self.add_branch(branch);
        }
        if let Some(repo) = &workspace.repo {
            self.add_own_name(repo);
        }
        self.add_paths(&workspace.commented);
        if let Some(title) = &workspace.pull_request {
            self.add_prose(title);
        }
    }

    /// Walks `log` newest-first with separate budgets for conversation messages and tool calls, so a turn with dozens
    /// of tool calls doesn't crowd out the names discussed a turn or two earlier.
    fn add_session_log(&mut self, log: &SessionLog) {
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

    fn add_short_label(&mut self, label: &str) {
        let clean = label.strip_prefix("(Recommended)").map_or(label, str::trim).trim();
        if clean.split_whitespace().count() <= 3 && !all_common_english(clean) {
            self.add_proper_noun(clean);
        }
    }

    fn add_branch(&mut self, branch: &str) {
        for seg in branch.split('/') {
            if !is_boilerplate(seg) {
                self.add_own_term(seg);
            }
        }
    }

    fn add_event(&mut self, body: &EventBody, text_messages: &mut usize, tool_events: &mut usize) {
        match body {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
                self.add_blocks(blocks, text_messages, tool_events);
            }
            EventBody::UserPrompt { prompt, from, .. } if from.is_person() && *text_messages < MAX_TEXT_MESSAGES => {
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
            | EventBody::ConversationStarted { .. }
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
            ToolCall::Read(ReadFile { path, .. })
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
            ToolCall::Typed(_) | ToolCall::Other(_) | ToolCall::MakeImage(_) => {
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

/// Whose a hint is: the workspace's own, such as its branch's words or a file's, which a person also says as words,
/// or another's, such as a package's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Whose {
    Own,
    Named,
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
