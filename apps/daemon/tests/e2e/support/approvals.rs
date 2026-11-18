//! The requests for approval a chat waits on, and answering them as a client does.

use client_core::Client;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ApprovalId, SessionId};

use super::stack::{read_all, wait_until};

pub(crate) fn pending_approval(events: &[SessionEvent]) -> Option<ApprovalId> {
    let mut open = None;
    for event in events {
        match &event.body {
            EventBody::ApprovalRequested { approval_id, .. } => open = Some(approval_id.clone()),
            EventBody::ApprovalResolved { approval_id, .. } if open.as_ref() == Some(approval_id) => open = None,
            _ => {}
        }
    }
    open
}

pub(crate) fn wait_for_approval(client: &Client, session_id: &SessionId) -> ApprovalId {
    let mut found = None;
    wait_until("an approval request", || {
        found = pending_approval(&read_all(client, session_id));
        found.is_some()
    });
    found.unwrap()
}

/// Approves the plan waiting in `session_id` under `approval`, as a client does.
pub(crate) fn approve_plan(client: &Client, session_id: &SessionId, approval: &ApprovalId) {
    let session = client.session_get(session_id).unwrap();
    let asked = client.waiting_request(&session, approval).unwrap().expect("the plan waits");
    let approved = client_core::Approval::of(&asked.body).unwrap().approve();
    client.session_decide(session_id, approval, approved.expect("a plan takes a yes")).unwrap();
}
