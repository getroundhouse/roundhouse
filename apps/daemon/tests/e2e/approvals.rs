//! Approvals.

#![cfg(test)]

use std::path::PathBuf;

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::event::{ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody};
use protocol::model::SessionStatus;

use crate::support::{Stack, wait_for_approval, wait_for_status, wait_for_turn_end};

#[test]
fn an_approval_can_be_answered_by_a_different_client_after_a_restart() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo approved > approved.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    wait_for_status(&client, &session.id, SessionStatus::WaitingApproval);
    let worktree = PathBuf::from(workspace.path.unwrap());
    assert!(!worktree.join("approved.txt").exists(), "must not run before it's approved");
    drop(client);

    let other = stack.client();
    other
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap();
    let events = wait_for_turn_end(&other, &session.id, 1);
    assert!(worktree.join("approved.txt").exists());
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::ApprovalResolved { approval_id: id, outcome: ApprovalOutcome::Allowed, .. } if *id == approval_id
    )));

    // Answering twice is an error rather than sending the agent a second response.
    let err = other
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
}

#[test]
fn a_denied_tool_does_not_run_and_the_agent_hears_why() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo nope > nope.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    client
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Deny { message: Some("not that".into()), interrupt: false },
        )
        .unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(!PathBuf::from(workspace.path.unwrap()).join("nope.txt").exists());
    let told = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { text, is_error: true, .. } if text == "not that")),
        _ => false,
    });
    assert!(told);
}

#[test]
fn an_approved_edit_to_a_tools_input_is_what_runs() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo original > out.txt", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    let decision = ApprovalDecision::Allow {
        updated_input: Some(serde_json::json!({"command": "echo edited > out.txt"})),
        updated_permissions: None,
    };
    client.session_approve(&session.id, &approval_id, decision).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let out = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("out.txt")).unwrap();
    assert_eq!(out, "edited\n");
}
