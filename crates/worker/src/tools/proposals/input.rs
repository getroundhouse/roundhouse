//! What the agent writes as `propose`'s arguments, which the tool's schema describes, read into what the hub keeps.

use protocol::model::{
    MAX_HEADER_CHARS, MAX_OPTIONS, MAX_QUESTIONS, MIN_OPTIONS, OptionPlace, ProposalOption, ProposalQuestion, Proposed,
    Recommendation,
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
}

/// The arguments the agent wrote, as what the hub keeps. The hub checks the rest, as `Proposed::check` says, so a
/// count or a length the schema gives isn't checked twice.
pub(super) fn proposed(params: &Map<String, Value>) -> Result<Proposed, String> {
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let written: Written = serde_json::from_value(arguments).map_err(|e| format!("the arguments don't fit: {e}"))?;
    let questions = written.questions.into_iter().enumerate().map(|(at, question)| question.read(at + 1));
    Ok(Proposed { title: written.title, context: written.context, questions: questions.collect::<Result<_, _>>()? })
}

impl WrittenQuestion {
    fn read(self, question: usize) -> Result<ProposalQuestion, String> {
        let recommended = match self.recommended.as_deref().map(str::trim).filter(|letter| !letter.is_empty()) {
            None => None,
            Some(letter) => {
                let option = OptionPlace::of_letter(letter)
                    .ok_or_else(|| format!("question {question} recommends {letter:?}, which isn't a letter"))?;
                Some(Recommendation { option, why: self.why })
            }
        };
        let options = self.options.into_iter().map(|o| ProposalOption {
            label: o.label,
            description: o.description,
            body: o.body,
        });
        Ok(ProposalQuestion {
            header: self.header,
            text: self.question,
            details: self.details,
            options: options.collect(),
            recommended,
        })
    }
}

/// The arguments' schema, which says what each part is for, since the agent reads it to write them.
pub(super) fn schema() -> Value {
    let markdown = "Markdown: paragraphs, lists, tables, code blocks, links, and images in the worktree, such as \
        `![How it flows](docs/flow.svg)`, by a path relative to it.";
    let option = json!({
        "type": "object",
        "properties": {
            "label": { "type": "string", "description": "A few words that name the option." },
            "description": { "type": "string", "description": "One line under the label that says what sets it apart." },
            "body": { "type": "string", "description": format!("The option in full: what it is, how it would work and what it costs. {markdown}") },
        },
        "required": ["label", "description", "body"],
        "additionalProperties": false,
    });
    let question = json!({
        "type": "object",
        "properties": {
            "header": { "type": "string", "maxLength": MAX_HEADER_CHARS, "description": "A few words that name the question in the outline and the answer bar, such as \"Storage\"." },
            "question": { "type": "string", "description": "What's being decided, as a question." },
            "details": { "type": "string", "description": format!("What the person needs between the question and the options, such as a table that compares them. {markdown}") },
            "options": { "type": "array", "items": option, "minItems": MIN_OPTIONS, "maxItems": MAX_OPTIONS, "description": "The options, read and named in this order, from A." },
            "recommended": { "type": "string", "enum": ["A", "B", "C", "D"], "description": "The option you recommend, by its letter. Leave it out when you have no view." },
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
