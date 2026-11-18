//! Typing each request for approval, what closed it, and the draft of an answer to one, in place, as `crate::untyped`
//! types them.

use std::collections::HashMap;

use protocol::capability::approvals::{Asks, Request};
use protocol::event::ContentBlock;
use protocol::model::{AnswerSoFar, ApprovalId, DecisionDraft, SoFar, ToolUseId, UseChoice};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::Value;

use crate::store::StoreError;
use crate::untyped::{type_request, type_resolution};

/// Each request for approval typed, with the fields of the older form beside a typed one gone, each resolution with
/// the decision it stood for, as `crate::untyped` reads it from the request it closed and the words a denied call's
/// result kept, and each chat's draft of an answer typed for what its request asks. Only a body that changes is
/// written, and an event the hub can't read is left as it is.
pub(super) fn typed_requests(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let sessions = tx
        .prepare("SELECT DISTINCT session_id FROM events WHERE kind IN ('approval_requested', 'approval_resolved')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for session in sessions {
        session_events(tx, &session)?;
    }
    drafts(tx)
}

/// What a session's requests asked, by their IDs, with the call each asked to make.
type Asked = HashMap<ApprovalId, (Asks, Option<ToolUseId>)>;

fn session_events(tx: &Transaction<'_>, session: &str) -> Result<(), StoreError> {
    let rows = tx
        .prepare(
            "SELECT seq, kind, body FROM events WHERE session_id = ?1
                AND (kind IN ('approval_requested', 'approval_resolved')
                    OR (kind = 'agent_user' AND instr(body, '\"denied\"') > 0))
                ORDER BY seq",
        )?
        .query_map([session], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE events SET body = ?3 WHERE session_id = ?1 AND seq = ?2")?;
    let (mut asked, mut said): (Asked, HashMap<ToolUseId, Option<String>>) = (HashMap::new(), HashMap::new());
    let mut resolutions = Vec::new();
    for (seq, kind, body) in rows {
        let Ok(mut body) = serde_json::from_str::<Value>(&body) else { continue };
        match kind.as_str() {
            "approval_requested" => {
                if type_request(&mut body) {
                    write.execute(params![session, seq, serde_json::to_string(&body)?])?;
                }
                if let Ok(typed) = Requested::deserialize(&body) {
                    asked.insert(typed.approval_id, (typed.request.asks, typed.tool_use_id));
                }
            }
            "agent_user" => said.extend(denied(&body)),
            _ => resolutions.push((seq, body)),
        }
    }
    // A denied call's result comes after what closed its request, so each resolution is typed once all are read.
    for (seq, mut body) in resolutions {
        let request = body.get("approval_id").and_then(|id| asked.get(&ApprovalId::from(id.as_str()?)));
        let words = request.and_then(|(_, call)| said.get(call.as_ref()?)?.as_deref());
        if type_resolution(&mut body, request.map(|(asks, _)| asks), words) {
            write.execute(params![session, seq, serde_json::to_string(&body)?])?;
        }
    }
    Ok(())
}

/// What a request typed holds that a resolution is typed by.
#[derive(Deserialize)]
struct Requested {
    approval_id: ApprovalId,
    request: Request,
    tool_use_id: Option<ToolUseId>,
}

/// What each denied call the agent's event `body` holds a result of was denied with, by the call's ID: the person's
/// words, or `None` where they said nothing.
fn denied(body: &Value) -> Vec<(ToolUseId, Option<String>)> {
    let blocks = body.get("blocks").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let results = blocks.iter().filter_map(|block| ContentBlock::deserialize(block).ok());
    results
        .filter_map(|block| {
            let ContentBlock::CallResult { tool_use_id, denied: Some(denied), .. } = block else { return None };
            Some((tool_use_id, denied.said))
        })
        .collect()
}

/// A draft of an answer in the form from before drafts were typed: what's chosen and typed on each question, where a
/// request to use a tool was asked as one question whose first option allows it and whose second denies it.
#[derive(Deserialize)]
struct Before {
    approval_id: ApprovalId,
    #[serde(default)]
    answers: Vec<AnswerSoFar>,
}

/// Each chat's draft of an answer typed, as what it drafts for what its request asks, under the name of what it holds
/// now, a decision's draft. One for a request it can't find, or that asks for a plan's approval, which no client
/// drafts, goes.
fn drafts(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch("ALTER TABLE sessions RENAME COLUMN answer_draft TO decision_draft")?;
    let drafts = tx
        .prepare("SELECT id, decision_draft FROM sessions WHERE decision_draft IS NOT NULL")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut asked = tx.prepare(
        "SELECT body FROM events WHERE session_id = ?1 AND kind = 'approval_requested'
            AND json_extract(body, '$.approval_id') = ?2 ORDER BY seq DESC LIMIT 1",
    )?;
    let mut write = tx.prepare("UPDATE sessions SET decision_draft = ?2 WHERE id = ?1")?;
    for (session, draft) in drafts {
        let typed = serde_json::from_str::<Before>(&draft).ok().and_then(|before| {
            let request: String =
                asked.query_row(params![session, before.approval_id.as_str()], |row| row.get(0)).optional().ok()??;
            let asks = serde_json::from_str::<Requested>(&request).ok()?.request.asks;
            drafted(before, &asks)
        });
        write.execute(params![session, typed.map(|draft| serde_json::to_string(&draft)).transpose()?])?;
    }
    Ok(())
}

/// `before`, a draft of an answer to a request that asks `asks`, typed.
fn drafted(before: Before, asks: &Asks) -> Option<DecisionDraft> {
    let Before { approval_id, answers } = before;
    let so_far = match asks {
        Asks::Questions { .. } => SoFar::Answers { answers },
        Asks::Plan { .. } => return None,
        Asks::Call { .. } | Asks::Network { .. } | Asks::Access { .. } | Asks::Server(_) | Asks::Unknown => {
            let AnswerSoFar { picked, typed, other } = answers.into_iter().next().unwrap_or_default();
            let chosen = match picked.first() {
                Some(0) => Some(UseChoice::Allow),
                Some(1) => Some(UseChoice::Deny),
                Some(_) => Some(UseChoice::Unknown),
                None => None,
            };
            SoFar::Use { chosen, words: typed, writing: other }
        }
    };
    Some(DecisionDraft { approval_id, so_far })
}
