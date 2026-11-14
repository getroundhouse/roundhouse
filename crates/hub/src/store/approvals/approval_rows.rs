use super::super::{Store, StoreError, lock};
use protocol::event::EventBody;
use protocol::model::{ApprovalId, OpenApproval, Seq, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

/// Notes what the event recorded at `seq` does to the approvals the agent is waiting on, as part of the transaction
/// that records it, and returns them if it changed them. One is open from its request until an event resolves it, as
/// the hub does when its agent goes. A request for one that's open already changes nothing, nor does resolving one
/// that isn't.
pub(in super::super) fn note_approvals(
    conn: &Connection,
    id: &SessionId,
    seq: Seq,
    body: &EventBody,
) -> Result<Option<Vec<OpenApproval>>, StoreError> {
    let (EventBody::ApprovalRequested { approval_id, .. } | EventBody::ApprovalResolved { approval_id, .. }) = body
    else {
        return Ok(None);
    };
    let mut open = open_on(conn, id)?;
    let was_open = open.iter().any(|approval| approval.approval_id == *approval_id);
    let requested = matches!(body, EventBody::ApprovalRequested { .. });
    if requested == was_open {
        return Ok(None);
    }
    if requested {
        open.push(OpenApproval { seq, approval_id: approval_id.clone() });
    } else {
        open.retain(|approval| approval.approval_id != *approval_id);
    }
    conn.execute(
        "UPDATE sessions SET open_approvals = ? WHERE id = ?",
        params![serde_json::to_string(&open)?, id.as_str()],
    )?;
    Ok(Some(open))
}

/// The approvals the session's agent is waiting on, oldest first, as its record has them.
fn open_on(conn: &Connection, id: &SessionId) -> Result<Vec<OpenApproval>, StoreError> {
    let json: String =
        conn.query_row("SELECT open_approvals FROM sessions WHERE id = ?", [id.as_str()], |row| row.get(0))?;
    Ok(serde_json::from_str(&json)?)
}

impl Store {
    /// The approvals requested and never resolved, oldest first, as the session's record has them.
    pub fn open_approvals(&self, session_id: &SessionId) -> Result<Vec<ApprovalId>, StoreError> {
        let open = open_on(&lock(&self.reader), session_id)?;
        Ok(open.into_iter().map(|approval| approval.approval_id).collect())
    }
}

/// The newest request bearing this ID, including one just answered, whose decision still has to reach its agent.
pub(in super::super) fn request_on(
    conn: &Connection,
    session: &SessionId,
    approval: &ApprovalId,
) -> Result<Option<EventBody>, StoreError> {
    let body: Option<String> = conn.query_row(
        "SELECT body FROM events WHERE session_id = ? AND kind = 'approval_requested' AND json_extract(body, '$.approval_id') = ? ORDER BY seq DESC LIMIT 1",
        params![session.as_str(), approval.as_str()], |row| row.get(0)).optional()?;
    Ok(body.map(|body| serde_json::from_str(&body)).transpose()?)
}

impl Store {
    pub fn approval_request(
        &self,
        session: &SessionId,
        approval: &ApprovalId,
    ) -> Result<Option<EventBody>, StoreError> {
        request_on(&lock(&self.reader), session, approval)
    }
}
