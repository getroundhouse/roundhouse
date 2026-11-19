use super::super::mcp::Called;
use super::described::describe;
use super::input::{proposed, schema};
use crate::asks::Asks;
use protocol::model::{AskKey, SessionId, ToolUseId};
use protocol::worker::{ProposalRecord, Propose, ReadProposal};
use serde_json::{Map, Value, json};

pub(in crate::tools) const PROPOSE: &str = "propose";
pub(in crate::tools) const GET: &str = "get_proposal";

/// When to propose rather than ask, and how, which the agent reads before it first calls it. *Unconfirmed.*
const PROPOSE_DESCRIPTION: &str = "Asks the person to decide something too large for a question, as a proposal they \
    read in a tab of its own in Roundhouse and answer when they're ready. Use it rather than asking in the chat, or \
    with a tool that asks questions, when an option needs more than a sentence to describe, or the person needs a \
    comparison, code or a picture to judge the options, such as choosing between architectures. Give it 1 to 4 \
    questions, each with 2 to 4 options, and each option a label, a one-line description and a body in \
    Markdown that says what it is and what it costs. Recommend one, and say why, when you have a view. It returns at \
    once: the person answers later, and their answer comes to you as a prompt. Don't ask them the same in the chat, \
    or wait or poll for the answer: carry on with other work, or end your turn.";

const GET_DESCRIPTION: &str = "Reads one of the proposals you made in this chat with the propose tool, by its ID, \
    which propose's result gives: its context, each question with its options and what you recommended, and the \
    person's answer once they've given it, with the note they wrote.";

pub(in crate::tools) fn definitions() -> Vec<Value> {
    let get = json!({
        "type": "object",
        "properties": { "proposal": { "type": "string", "description": "The proposal's ID." } },
        "required": ["proposal"],
        "additionalProperties": false,
    });
    vec![
        json!({
            "name": PROPOSE,
            "title": "Propose",
            "description": PROPOSE_DESCRIPTION,
            "inputSchema": schema(),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": GET,
            "title": "Get a proposal",
            "description": GET_DESCRIPTION,
            "inputSchema": get,
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        }),
    ]
}

/// Makes the proposal the agent wrote, keyed by its call when its CLI names it, so a hub asked again makes it once,
/// and by a key of its own otherwise.
pub(in crate::tools) async fn propose(asks: &Asks, session_id: &SessionId, params: &Map<String, Value>) -> Called {
    let proposed = match proposed(params) {
        Ok(proposed) => proposed,
        Err(problem) => return Called::failed(format!("Roundhouse couldn't read the proposal: {problem}")),
    };
    let call: Option<ToolUseId> =
        params.get("_meta").and_then(|meta| meta.get("claudecode/toolUseId")).and_then(Value::as_str).map(Into::into);
    let key = call.as_ref().map_or_else(|| AskKey::from(uuid::Uuid::new_v4().to_string()), |call| call.as_str().into());
    match asks.ask(session_id.clone(), Propose { proposed, call, key }).await {
        Ok(ProposalRecord { proposal }) => Called::text(format!(
            "Proposed \"{}\" as {}. It's open in a tab for the person, who answers when they're ready, and their \
             answer comes to you as a prompt. Don't ask them the same in the chat, or wait or poll for it: carry on \
             with other work, or end your turn. get_proposal reads it back.",
            proposal.current.proposed.title, proposal.id
        )),
        Err(e) => Called::failed(format!("Roundhouse couldn't make the proposal: {}", e.message)),
    }
}

pub(in crate::tools) async fn get(asks: &Asks, session_id: &SessionId, params: &Map<String, Value>) -> Called {
    let id = params.get("arguments").and_then(|args| args.get("proposal")).and_then(Value::as_str);
    let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Called::failed("give the proposal's ID, as propose's result said it".to_string());
    };
    match asks.ask(session_id.clone(), ReadProposal { id: id.into() }).await {
        Ok(ProposalRecord { proposal }) => Called::text(describe(&proposal)),
        Err(e) => Called::failed(format!("Roundhouse couldn't read the proposal: {}", e.message)),
    }
}
