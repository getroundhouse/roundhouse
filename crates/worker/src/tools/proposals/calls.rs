use super::super::mcp::Called;
use super::described::{describe, responded_before};
use super::input::{proposed, revise_schema, revised, schema};
use crate::asks::Asks;
use crate::tools::Caller;
use protocol::model::{AskKey, Proposal, RevisionNumber, SessionId, ToolUseId};
use protocol::worker::{ProposalRecord, Propose, ReadProposal, Revise};
use serde_json::{Map, Value, json};

pub(in crate::tools) const PROPOSE: &str = "propose";
pub(in crate::tools) const REVISE: &str = "revise_proposal";
pub(in crate::tools) const GET: &str = "get_proposal";

/// When to propose rather than ask, and how, which the agent reads before it first calls it. *Unconfirmed.*
const PROPOSE_DESCRIPTION: &str = "Asks the person to decide something too large for a question, as a proposal they \
    read in a tab of its own in Roundhouse and answer when they're ready. Use it rather than asking in the chat, or \
    with a tool that asks questions, when an option needs more than a sentence to describe, or the person needs a \
    comparison, code or a picture to judge the options, such as choosing between architectures. Give it 1 to 4 \
    questions, each with 2 to 4 options, and each option a label, a one-line description and a body in \
    Markdown that says what it is and what it costs. Draw a diagram in Mermaid, in a code block whose language is \
    `mermaid`. To show another picture, such as a screenshot, save it in the worktree and point to it with a Markdown \
    image: Roundhouse keeps a copy with the proposal, as the file is when you propose. Recommend one, and say why, \
    when you have a view. It returns at once: the person answers later, and their answer, or their request for \
    another round, which you answer with revise_proposal, comes to you as a prompt. Don't ask them the same in the chat, or wait or poll for the answer: carry on with other work, or end your turn.";

/// When to revise a proposal, and how. *Unconfirmed.*
const REVISE_DESCRIPTION: &str = "Writes a new revision of a proposal you made in this chat with the propose tool, \
    by its ID, whether or not the person has answered the newest, such as when they asked for another round, or you \
    learned something that changes the options. Write the whole proposal again, as you would to propose it, and say \
    what changed since the revision before. Give each option you keep the letter it had, where that makes sense, so \
    the person names it as they did: dropping B leaves C as C. The person reads the newest revision in its tab, and an \
    answer to an earlier one they hadn't sent is refused. It returns at once, as propose does.";

const GET_DESCRIPTION: &str = "Reads one of the proposals you made in this chat with the propose tool, by its ID, \
    which propose's result gives: its context, each question with its options and what you recommended, and the \
    person's answer, or request for another round, once they've given it, with the note they wrote. It reads the \
    newest revision, or the one you name, such as the one the person's answer names.";

pub(in crate::tools) fn definitions() -> Vec<Value> {
    let get = json!({
        "type": "object",
        "properties": {
            "proposal": { "type": "string", "description": "The proposal's ID." },
            "revision": { "type": "integer", "minimum": 1, "description": "Which revision to read, when the person's answer named one. Leave it out for the newest." },
        },
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
            "name": REVISE,
            "title": "Revise a proposal",
            "description": REVISE_DESCRIPTION,
            "inputSchema": revise_schema(),
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
/// and by a key of its own otherwise, with the images what it wrote points to in its worktree, as they are now.
pub(in crate::tools) async fn propose(asks: &Asks, caller: &Caller, params: &Map<String, Value>) -> Called {
    let proposed = match proposed(params) {
        Ok(proposed) => proposed,
        Err(problem) => return Called::failed(format!("Roundhouse couldn't read the proposal: {problem}")),
    };
    let call = call_of(params);
    let key = key_of(call.as_ref());
    let written = proposed.markdown().map(str::to_string).collect();
    let (images, attachments) = crate::linked_images::keep_for(&caller.worktree, written).await;
    let asked = Propose { proposed, call, key, images, attachments };
    match asks.ask(caller.session_id.clone(), asked).await {
        Ok(ProposalRecord { proposal, .. }) => Called::text(format!(
            "Proposed \"{}\" as {}. It's open in a tab for the person, who answers when they're ready, and their \
             answer comes to you as a prompt. Don't ask them the same in the chat, or wait or poll for it: carry on \
             with other work, or end your turn. get_proposal reads it back.",
            proposal.current.proposed.title, proposal.id
        )),
        Err(e) => Called::failed(format!("Roundhouse couldn't make the proposal: {}", e.message)),
    }
}

/// Writes the revision the agent wrote of one of its proposals, keyed as `propose` keys a proposal, with the images
/// what it wrote points to in its worktree, as they are now. Its reply says when the person had responded to the
/// revision before, since the prompt that says so may not have reached the agent yet.
pub(in crate::tools) async fn revise(asks: &Asks, caller: &Caller, params: &Map<String, Value>) -> Called {
    let (id, changed, proposed) = match revised(params) {
        Ok(revised) => revised,
        Err(problem) => return Called::failed(format!("Roundhouse couldn't read the revision: {problem}")),
    };
    let call = call_of(params);
    let key = key_of(call.as_ref());
    let written = proposed.markdown().map(str::to_string).collect();
    let (images, attachments) = crate::linked_images::keep_for(&caller.worktree, written).await;
    let asked = Revise { id, proposed, changed, call, key, images, attachments };
    match asks.ask(caller.session_id.clone(), asked).await {
        Ok(ProposalRecord { proposal, .. }) => Called::text(revised_in_words(&proposal)),
        Err(e) => Called::failed(format!("Roundhouse couldn't revise the proposal: {}", e.message)),
    }
}

/// What the agent is told once it's revised: which revision it wrote, and what the person said to the one before, if
/// they'd said anything.
fn revised_in_words(proposal: &Proposal) -> String {
    let responded = responded_before(proposal).map_or_else(String::new, |responded| format!(" {responded}"));
    format!(
        "Revised \"{}\" as revision {} of {}. The person reads it in its tab, and their answer comes to you as a \
         prompt. Carry on with other work, or end your turn.{responded}",
        proposal.current.proposed.title, proposal.current.number, proposal.id
    )
}

/// The agent's call, when its CLI names it.
fn call_of(params: &Map<String, Value>) -> Option<ToolUseId> {
    params.get("_meta").and_then(|meta| meta.get("claudecode/toolUseId")).and_then(Value::as_str).map(Into::into)
}

/// The key the hub knows an ask by: the agent's call, or a key of its own when its CLI doesn't name it.
fn key_of(call: Option<&ToolUseId>) -> AskKey {
    call.map_or_else(|| AskKey::from(uuid::Uuid::new_v4().to_string()), |call| call.as_str().into())
}

pub(in crate::tools) async fn get(asks: &Asks, session_id: &SessionId, params: &Map<String, Value>) -> Called {
    let id = params.get("arguments").and_then(|args| args.get("proposal")).and_then(Value::as_str);
    let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Called::failed("give the proposal's ID, as propose's result said it".to_string());
    };
    let revision = params.get("arguments").and_then(|args| args.get("revision")).and_then(Value::as_u64);
    let revision = revision.and_then(|number| u32::try_from(number).ok()).map(RevisionNumber);
    match asks.ask(session_id.clone(), ReadProposal { id: id.into(), revision }).await {
        Ok(ProposalRecord { proposal, revision }) => Called::text(describe(&proposal, revision.as_deref())),
        Err(e) => Called::failed(format!("Roundhouse couldn't read the proposal: {}", e.message)),
    }
}
