//! What the agent writes as `propose`'s arguments, which the tool's schema describes, read into what the hub keeps.

use protocol::model::ProposalId;
use protocol::model::{
    MAX_HEADER_CHARS, MAX_OPTIONS, MAX_QUESTIONS, MIN_OPTIONS, OptionLetter, ProposalOption, ProposalQuestion,
    Proposed, Recommendation,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// What's unknown at the top is ignored, as the other tools ignore it, since a CLI can add arguments of its own to
/// every tool, as `agy` adds `toolSummary`. Inside a question or an option, it's the agent's mistake, and refused.
#[derive(Deserialize)]
struct Written {
    title: String,
    #[serde(default)]
    context: String,
    questions: Vec<WrittenQuestion>,
}

/// What the agent writes for one question. It writes the question as `question`, the word Claude's own question tool
/// uses, which the hub keeps as its `text`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenQuestion {
    header: String,
    question: String,
    #[serde(default)]
    details: String,
    options: Vec<WrittenOption>,
    #[serde(default)]
    recommended: Option<String>,
    #[serde(default)]
    why: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenOption {
    label: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    letter: Option<String>,
}

/// What the agent writes as `revise_proposal`'s arguments: the proposal it revises and what changed, besides what it
/// writes to propose.
#[derive(Deserialize)]
struct WrittenRevision {
    proposal: String,
    changed: String,
    #[serde(flatten)]
    written: Written,
}

/// The arguments the agent wrote to revise a proposal: which, what changed, and the revision, as what the hub keeps.
pub(super) fn revised(params: &Map<String, Value>) -> Result<(ProposalId, String, Proposed), String> {
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let revision: WrittenRevision =
        serde_json::from_value(arguments).map_err(|e| format!("the arguments don't fit: {e}"))?;
    let id = revision.proposal.trim();
    if id.is_empty() {
        return Err("give the proposal's ID, as propose's result said it".to_string());
    }
    Ok((id.into(), revision.changed, revision.written.read()?))
}

/// The arguments the agent wrote, as what the hub keeps. The hub checks the rest, as `Proposed::check` says, so a
/// count or a length the schema gives isn't checked twice.
///
/// An option's letter is only for keeping the one it had when revising, as `revise_proposal`'s schema says, so a new
/// proposal that names one is refused, rather than taking what `propose`'s schema leaves out.
pub(super) fn proposed(params: &Map<String, Value>) -> Result<Proposed, String> {
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let written: Written = serde_json::from_value(arguments).map_err(|e| format!("the arguments don't fit: {e}"))?;
    let proposed = written.read()?;
    let named = proposed.questions.iter().flat_map(|question| &question.options).any(|option| option.letter.is_some());
    if named {
        return Err("a new proposal's options take the letters A, B, C in order, so leave out letter, which only \
                    revise_proposal takes, to keep an option's letter"
            .to_string());
    }
    Ok(proposed)
}

impl Written {
    fn read(self) -> Result<Proposed, String> {
        let questions = self.questions.into_iter().enumerate().map(|(at, question)| question.read(at + 1));
        Ok(Proposed { title: self.title, context: self.context, questions: questions.collect::<Result<_, _>>()? })
    }
}

impl WrittenQuestion {
    fn read(self, question: usize) -> Result<ProposalQuestion, String> {
        let options = self.options.into_iter().map(|o| {
            let letter = match o.letter.as_deref().map(str::trim).filter(|letter| !letter.is_empty()) {
                None => None,
                Some(written) => Some(OptionLetter::read(written).ok_or_else(|| {
                    format!("question {question} names an option {written:?}, which isn't a letter from A to Z")
                })?),
            };
            Ok(ProposalOption { label: o.label, description: o.description, body: o.body, letter })
        });
        let mut read = ProposalQuestion {
            header: self.header,
            text: self.question,
            details: self.details,
            options: options.collect::<Result<_, String>>()?,
            recommended: None,
        };
        if let Some(written) = self.recommended.as_deref().map(str::trim).filter(|letter| !letter.is_empty()) {
            let letter = OptionLetter::read(written)
                .ok_or_else(|| format!("question {question} recommends {written:?}, which isn't a letter"))?;
            let option = read
                .place_of(letter)
                .ok_or_else(|| format!("question {question} recommends option {letter}, which it doesn't have"))?;
            read.recommended = Some(Recommendation { option, why: self.why });
        }
        Ok(read)
    }
}

/// The arguments' schema for proposing, which says what each part is for, since the agent reads it to write them.
pub(super) fn schema() -> Value {
    written_schema(false)
}

/// The arguments' schema for revising: which proposal, what changed, and the revision as it would be proposed, with
/// a letter for each option that keeps the one it had.
pub(super) fn revise_schema() -> Value {
    let mut schema = written_schema(true);
    schema["properties"]["proposal"] =
        json!({ "type": "string", "description": "The proposal's ID, as propose's result said it." });
    schema["properties"]["changed"] = json!({ "type": "string", "description": "What changed since the revision before, in a sentence or two, such as which options you dropped, added or changed, and why. The person reads it first." });
    schema["required"] = json!(["proposal", "changed", "title", "context", "questions"]);
    schema
}

fn written_schema(revising: bool) -> Value {
    let markdown = "Markdown: paragraphs, lists, tables, code blocks, links, diagrams in a code block whose language \
        is `mermaid`, and images and videos in the worktree, such as `![How it flows](docs/flow.svg)` or \
        `![The new flow](.context/recordings/flow.webm)`, by a path relative to it.";
    let mut option = json!({
        "type": "object",
        "properties": {
            "label": { "type": "string", "description": "A few words that name the option." },
            "description": { "type": "string", "description": "One line under the label that says what sets it apart." },
            "body": { "type": "string", "description": format!("The option in full: what it is, how it would work and what it costs. {markdown}") },
        },
        "required": ["label", "description", "body"],
        "additionalProperties": false,
    });
    if revising {
        option["properties"]["letter"] = json!({ "type": "string", "pattern": "^[A-Z]$", "description": "The letter it had in the revision before, so the person names it as they did, such as C when you dropped B. Prefer keeping each option's letter where it makes sense. Leave it out for a new option, which takes the first letter no other option has." });
    }
    let options = if revising {
        "The options, read in this order, each named by its letter."
    } else {
        "The options, read and named in this order, from A."
    };
    let question = json!({
        "type": "object",
        "properties": {
            "header": { "type": "string", "maxLength": MAX_HEADER_CHARS, "description": "A few words that name the question in the outline and the answer bar, such as \"Storage\"." },
            "question": { "type": "string", "description": "What's being decided, as a question." },
            "details": { "type": "string", "description": format!("What the person needs between the question and the options, such as a table that compares them. {markdown}") },
            "options": { "type": "array", "items": option, "minItems": MIN_OPTIONS, "maxItems": MAX_OPTIONS, "description": options },
            "recommended": { "type": "string", "pattern": "^[A-Z]$", "description": "The option you recommend, by its letter. Leave it out when you have no view." },
            "why": { "type": "string", "description": format!("Why you recommend it. {markdown}") },
        },
        "required": ["header", "question", "options"],
        "additionalProperties": false,
    });
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "What it's about, in a few words, which names its tab." },
            "context": { "type": "string", "description": format!("Before the questions: what's being decided, why now, and what the person needs to know to decide it. {markdown}") },
            "questions": { "type": "array", "items": question, "minItems": 1, "maxItems": MAX_QUESTIONS, "description": "Each question the person answers, in the order they read and answer them." },
        },
        "required": ["title", "context", "questions"],
        "additionalProperties": false,
    })
}
