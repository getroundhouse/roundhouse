#![cfg(test)]
use protocol::capability::calls::{
    Call, CallOutput,
    call::{Command, Replaces},
    output::{Ended, Lines},
};
use protocol::capability::setup::ServerName;
use serde_json::json;

use crate::acp::{ToolCallWire, ToolKindWire, ToolStatusWire};
use crate::antigravity::calls::{translate_call, translate_output};

#[test]
fn command_read_and_mcp_calls_translate_cleanly_and_strip_agy_wrappers() {
    let cmd_wire = ToolCallWire {
        tool_call_id: "c1".into(),
        status: Some(ToolStatusWire::Completed),
        kind: Some(ToolKindWire::Execute),
        title: Some("echo hi".into()),
        name: Some("run_command".into()),
        args: Some(json!({ "CommandLine": "echo hi", "toolSummary": "Print hi", "WaitMsBeforeAsync": 5000 })),
        raw_output: Some(json!({
            "exit_code": 0,
            "output": "Created At: 2026-11-14T19:00:00Z\nCompleted At: 2026-11-14T19:00:01Z\nCommand exited with code 0.\n\nCommand output:\nhi\n"
        })),
        ..ToolCallWire::default()
    };
    let call = translate_call(&cmd_wire);
    let Call::Command(cmd) = &call else { panic!("expected command call") };
    assert_eq!(cmd.command, "echo hi");
    assert_eq!(cmd.description.as_deref(), Some("Print hi"));
    let CallOutput::Command(out) = translate_output(&call, &cmd_wire, false) else { panic!("expected command out") };
    assert_eq!(out.output, "hi\n");
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.ended, Ended::Exited);

    let read_wire = ToolCallWire {
        tool_call_id: "r1".into(),
        name: Some("read_file".into()),
        args: Some(json!({ "Path": "/work/a.txt", "StartLine": 2, "EndLine": 3 })),
        content_text: Some("Created At: 2026-11-14T19:00:00Z\nCompleted At: 2026-11-14T19:00:00Z\nShowing lines 2-3 of 5\n2: two\n3: three".into()),
        ..ToolCallWire::default()
    };
    let rcall = translate_call(&read_wire);
    let CallOutput::Lines(lines) = translate_output(&rcall, &read_wire, false) else { panic!("expected lines") };
    assert_eq!(lines, Lines { first: 2, lines: vec!["two".into(), "three".into()], total: Some(5) });

    let mcp_wire = ToolCallWire {
        tool_call_id: "m1".into(),
        name: Some("mcp_roundhouse_get_pull_requests".into()),
        args: Some(json!({ "pull_requests": [42], "toolSummary": "Read PR #42" })),
        ..ToolCallWire::default()
    };
    assert_eq!(
        translate_call(&mcp_wire),
        Call::Mcp {
            server: ServerName::from("roundhouse"),
            tool: "get_pull_requests".into(),
            input: json!({ "pull_requests": [42] }),
        }
    );
}

#[test]
fn string_command_output_and_underscore_mcp_servers_preserve_their_fields() {
    let cmd_wire = ToolCallWire {
        tool_call_id: "c2".into(),
        status: Some(ToolStatusWire::Failed),
        name: Some("run_command".into()),
        args: Some(json!({ "CommandLine": "exit 7" })),
        raw_output: Some(json!("Command exited with code 7.\n\nCommand output:\nboom\n")),
        ..ToolCallWire::default()
    };
    let call = translate_call(&cmd_wire);
    let CallOutput::Command(out) = translate_output(&call, &cmd_wire, false) else { panic!("expected command out") };
    assert_eq!(out.output, "boom\n");
    assert_eq!(out.exit_code, Some(7));

    let mcp_wire = ToolCallWire {
        tool_call_id: "m2".into(),
        title: Some("my_server: fetch_item".into()),
        name: Some("mcp_my_server_fetch_item".into()),
        args: Some(json!({ "id": 9, "toolSummary": "Fetch item" })),
        ..ToolCallWire::default()
    };
    assert_eq!(
        translate_call(&mcp_wire),
        Call::Mcp { server: ServerName::from("my_server"), tool: "fetch_item".into(), input: json!({ "id": 9 }) }
    );
}

#[test]
fn what_agy_does_its_own_way_is_told_as_it_is_rather_than_as_another_kind_does_it() {
    let wire = |name: &str, args: serde_json::Value| ToolCallWire {
        tool_call_id: "c".into(),
        name: Some(name.into()),
        args: Some(args),
        ..ToolCallWire::default()
    };

    let message = translate_call(&wire("send_message", json!({ "Recipient": "conv-7", "Message": "Carry on" })));
    let kill = translate_call(&wire("manage_subagents", json!({ "Action": "kill_all" })));
    let append = translate_call(&wire("write_file", json!({ "Path": "/w/log", "Content": "more", "Append": true })));
    let command = translate_call(&wire("run_command", json!({ "CommandLine": "make", "WaitMsBeforeAsync": 500 })));
    let edit = translate_call(&wire("edit_file", json!({ "Path": "/w/a", "Target": "x", "Replacement": "y" })));

    assert!(
        matches!(message, Call::Other { tool, .. } if tool == "send_message"),
        "a conversation's ID isn't a call's"
    );
    assert!(matches!(kill, Call::Other { tool, .. } if tool == "manage_subagents"));
    assert!(matches!(append, Call::Other { tool, .. } if tool == "write_file"), "an append doesn't write it whole");
    let Call::Command(Command { timeout_ms, .. }) = command else { panic!("{command:?}") };
    assert_eq!(timeout_ms, None, "how long it waits before backgrounding a command isn't a time limit");
    let Call::Edit(edit) = edit else { panic!("{edit:?}") };
    assert_eq!(edit.edits[0].replaces, Replaces::Every, "agy replaces every place its target is");
}
