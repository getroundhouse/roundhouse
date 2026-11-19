#![cfg(test)]
use protocol::capability::calls::{
    Call, CallOutput,
    call::{Command, CommandPart, MessageSubagent, Replaces, StartSubagent, StopSubagent},
    output::{Ended, Lines},
};
use protocol::capability::setup::ServerName;
use protocol::model::ModelName;
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
    let CallOutput::Command(out) = translate_output(&call, &cmd_wire, false, false) else {
        panic!("expected command out")
    };
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
    let CallOutput::Lines(lines) = translate_output(&rcall, &read_wire, false, false) else { panic!("expected lines") };
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
    let CallOutput::Command(out) = translate_output(&call, &cmd_wire, false, false) else {
        panic!("expected command out")
    };
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

/// `agy` names a subagent by the ID of its conversation, which `invoke_subagent` gives it, so a message to one, or a
/// stop of one, names it so. Listing them, or stopping several, isn't about one.
#[test]
fn a_message_to_a_subagent_or_a_stop_of_one_names_it_by_its_conversation() {
    let wire = |name: &str, args: serde_json::Value| ToolCallWire {
        tool_call_id: "c".into(),
        name: Some(name.into()),
        args: Some(args),
        ..ToolCallWire::default()
    };
    let manage = |args| translate_call(&wire("manage_subagents", args));

    let message = translate_call(&wire("send_message", json!({ "Recipient": "conv-7", "Message": "Carry on" })));
    let stop = manage(json!({ "Action": "kill", "ConversationIds": ["conv-7"] }));

    let to = MessageSubagent { subagent: "conv-7".into(), message: "Carry on".into() };
    assert_eq!(message, Call::MessageSubagent(to));
    assert_eq!(stop, Call::StopSubagent(StopSubagent { subagent: "conv-7".into() }));
    let several = manage(json!({ "Action": "kill", "ConversationIds": ["conv-7", "conv-8"] }));
    assert!(matches!(several, Call::Other { .. }), "{several:?}");
    assert!(matches!(manage(json!({ "Action": "list" })), Call::Other { .. }));
}

#[test]
fn what_agy_does_its_own_way_is_told_as_it_is_rather_than_as_another_kind_does_it() {
    let wire = |name: &str, args: serde_json::Value| ToolCallWire {
        tool_call_id: "c".into(),
        name: Some(name.into()),
        args: Some(args),
        ..ToolCallWire::default()
    };

    let message = translate_call(&wire("send_message", json!({ "Recipient": "@children", "Message": "Carry on" })));
    let kill = translate_call(&wire("manage_subagents", json!({ "Action": "kill_all" })));
    let append = translate_call(&wire("write_file", json!({ "Path": "/w/log", "Content": "more", "Append": true })));
    let command = translate_call(&wire("run_command", json!({ "CommandLine": "make", "WaitMsBeforeAsync": 500 })));
    let edit = translate_call(&wire("edit_file", json!({ "Path": "/w/a", "Target": "x", "Replacement": "y" })));

    assert!(matches!(message, Call::Other { tool, .. } if tool == "send_message"), "to every subagent at once");
    assert!(matches!(kill, Call::Other { tool, .. } if tool == "manage_subagents"), "every subagent at once");
    assert!(matches!(append, Call::Other { tool, .. } if tool == "write_file"), "an append doesn't write it whole");
    let Call::Command(Command { timeout_ms, .. }) = command else { panic!("{command:?}") };
    assert_eq!(timeout_ms, None, "how long it waits before backgrounding a command isn't a time limit");
    let Call::Edit(edit) = edit else { panic!("{edit:?}") };
    assert_eq!(edit.edits[0].replaces, Replaces::Every, "agy replaces every place its target is");
}

/// A command says what it does from the command it was given, and nothing from a title alone, which needn't be it.
#[test]
fn a_command_says_what_it_does_from_its_own_words() {
    let wire = |args: serde_json::Value, title: &str| ToolCallWire {
        tool_call_id: "c".into(),
        title: Some(title.into()),
        name: Some("run_command".into()),
        args: Some(args),
        ..ToolCallWire::default()
    };
    let parts = |call: Call| {
        let Call::Command(command) = call else { panic!("{call:?}") };
        command.parts
    };

    let reads = translate_call(&wire(json!({ "CommandLine": "cat a.rs" }), "Read a.rs"));
    let deletes = translate_call(&wire(json!({ "CommandLine": "cat a.rs | xargs rm" }), "Read a.rs"));
    let titled = translate_call(&wire(json!({}), "cat a.rs"));
    assert_eq!(parts(reads), [CommandPart::Read { path: "a.rs".into() }]);
    assert_eq!(parts(deletes), [CommandPart::Other]);
    assert_eq!(parts(titled), [], "a title alone says nothing of what it does");
}

#[test]
fn stopped_commands_distinguish_kills_and_cancels_from_exits_denials_and_spawn_errors() {
    let cmd_wire = |raw_output: Option<serde_json::Value>| ToolCallWire {
        tool_call_id: "c".into(),
        status: Some(ToolStatusWire::Failed),
        name: Some("run_command".into()),
        args: Some(json!({ "CommandLine": "sleep 30" })),
        raw_output,
        ..ToolCallWire::default()
    };
    let call = translate_call(&cmd_wire(None));
    let out = |wire: &ToolCallWire, cancelling: bool, denied: bool| {
        let out = translate_output(&call, wire, cancelling, denied);
        let CallOutput::Command(out) = out else { panic!("expected command output, got {out:?}") };
        out
    };

    let killed = out(&cmd_wire(Some(json!({ "output": "" }))), false, false);
    assert_eq!((killed.ended, killed.exit_code), (Ended::Stopped, None), "manage_task kill sends empty output");

    let cancelled = out(&cmd_wire(Some(json!({ "output": "partial\n" }))), true, false);
    assert_eq!((cancelled.ended, cancelled.exit_code), (Ended::Stopped, None));

    let silent_exit = out(
        &cmd_wire(Some(json!({
            "exit_code": 1,
            "output": "Created At: 2026-11-14T19:00:00Z\nCompleted At: 2026-11-14T19:00:01Z\nCommand exited with code 1.\nNo output from the command."
        }))),
        false,
        false,
    );
    assert_eq!((silent_exit.ended, silent_exit.exit_code, silent_exit.output.as_str()), (Ended::Exited, Some(1), ""));

    let exited_during_cancel =
        out(&cmd_wire(Some(json!("Command exited with code 7.\n\nCommand output:\nboom\n"))), true, false);
    assert_eq!((exited_during_cancel.ended, exited_during_cancel.exit_code), (Ended::Exited, Some(7)));

    let denied_during_cancel = out(&cmd_wire(Some(json!({ "output": "" }))), true, true);
    assert_eq!(denied_during_cancel.ended, Ended::Exited);

    let spawn_err = out(&cmd_wire(Some(json!("fork/exec /bin/zsh: resource temporarily unavailable"))), false, false);
    assert_eq!((spawn_err.ended, spawn_err.exit_code), (Ended::Exited, None));

    let no_payload = out(&cmd_wire(None), false, false);
    assert_eq!((no_payload.ended, no_payload.exit_code), (Ended::Exited, None));

    let null_payload = out(&cmd_wire(Some(serde_json::Value::Null)), false, false);
    assert_eq!((null_payload.ended, null_payload.exit_code, null_payload.output.as_str()), (Ended::Exited, None, ""));

    let json_err = out(&cmd_wire(Some(json!({ "error": "invalid arguments" }))), false, false);
    assert_eq!(
        (json_err.ended, json_err.exit_code, json_err.output.as_str()),
        (Ended::Exited, None, "{\"error\":\"invalid arguments\"}")
    );
}

#[test]
fn invoke_subagent_populates_explicit_model_and_leaves_inherit_or_absent_unset() {
    let model_of = |args: serde_json::Value| {
        let call = translate_call(&ToolCallWire {
            tool_call_id: "s".into(),
            name: Some("invoke_subagent".into()),
            args: Some(args),
            ..ToolCallWire::default()
        });
        let Call::Subagent(StartSubagent { model, .. }) = call else { panic!("{call:?}") };
        model
    };

    assert_eq!(model_of(json!({ "Prompt": "go", "Model": "flash" })), Some(ModelName::named("flash")));
    assert_eq!(model_of(json!({ "Prompt": "go", "Model": "pro" })), Some(ModelName::named("pro")));
    assert_eq!(model_of(json!({ "Prompt": "go", "Model": "inherit" })), None);
    assert_eq!(model_of(json!({ "Prompt": "go", "Model": "" })), None);
    assert_eq!(model_of(json!({ "Prompt": "go" })), None);
}
