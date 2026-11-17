//! That what Claude's adapter recorded untyped is typed as it records it now.

use protocol::capability::calls::call::{Command, Edit, Read, Replace, Replaces, Runs};
use protocol::capability::calls::output::{CommandOutput, Ended, FileOutcome, Lines};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{Asking, ContentBlock, FileChange, Question};
use protocol::model::{Lately, LatelyCall, LatelyResult, Said};
use protocol::worker::WorkerFrame;
use serde_json::{Value, json};

use super::{Typing, lately};

fn tool_use(id: &str, name: &str, input: &Value) -> Value {
    json!({"type":"tool_use","id":id,"name":name,"input":input})
}

/// What came back for `id`, as the adapter kept it once it said what kind of call each answered.
fn tool_result(id: &str, kind: &str, text: &str) -> Value {
    json!({"type":"tool_result","tool_use_id":id,"is_error":false,"text":text,"call":{"kind":kind}})
}

fn message(kind: &str, blocks: &[Value]) -> Value {
    json!({"type":kind,"blocks":blocks})
}

/// The blocks `bodies` come to, typed in order on one [`Typing`].
fn typed(bodies: Vec<Value>) -> Vec<ContentBlock> {
    let mut typing = Typing::default();
    bodies
        .into_iter()
        .flat_map(|mut body| {
            typing.body(&mut body);
            serde_json::from_value::<Vec<ContentBlock>>(body["blocks"].take()).unwrap()
        })
        .collect()
}

/// What came back for a call to `tool` with `input`, as `result` says it.
fn output(tool: &str, input: &Value, result: Value) -> CallOutput {
    let blocks = typed(vec![message("assistant", &[tool_use("c", tool, input)]), message("agent_user", &[result])]);
    let [_, ContentBlock::CallResult { output, .. }] = blocks.as_slice() else { panic!("{blocks:?}") };
    output.clone()
}

fn bash(command: &str) -> Call {
    Call::Command(Command {
        command: command.into(),
        cwd: None,
        description: None,
        timeout_ms: None,
        runs: Runs::Waits,
        parts: Vec::new(),
        unsandboxed: None,
    })
}

fn printed(output: &str, exit_code: Option<i32>) -> CallOutput {
    CallOutput::Command(CommandOutput {
        output: output.into(),
        exit_code,
        ended: Ended::Exited,
        took_ms: None,
        truncated: None,
    })
}

fn words(text: &str) -> CallOutput {
    CallOutput::Words { text: text.into() }
}

/// A call and its result come in events of their own, and the result is read against the call, whose kind of output it
/// is. What isn't a call or its result is left as it was.
#[test]
fn each_call_is_typed_and_its_result_read_against_it_from_an_earlier_event() {
    let read = tool_use("read", "Read", &json!({"file_path": "/a.rs"}));
    let said = json!({"type":"text","text":"I'll read it."});
    let lines = "     3\tfn a() {}\n     4\t}";

    let blocks = typed(vec![
        message("assistant", &[said.clone(), read]),
        message("agent_user", &[tool_result("read", "read", lines)]),
    ]);

    let made = Call::Read(Read { path: "/a.rs".into(), lines: None, pages: None });
    let came = CallOutput::Lines(Lines { first: 3, lines: vec!["fn a() {}".into(), "}".into()], total: None });
    assert_eq!(
        blocks,
        [
            serde_json::from_value(said).unwrap(),
            ContentBlock::Call { id: "read".into(), call: made },
            ContentBlock::CallResult {
                tool_use_id: "read".into(),
                output: came,
                is_error: false,
                denied: None,
                images: Vec::new()
            },
        ]
    );
}

#[test]
fn a_body_with_nothing_untyped_in_it_is_left_as_it_is() {
    let call = serde_json::to_value(ContentBlock::Call { id: "c".into(), call: bash("ls") }).unwrap();
    let mut typing = Typing::default();
    for body in [message("assistant", &[call]), json!({"type":"agent_other","line":"tool_use"})] {
        let mut kept = body.clone();
        assert!(!typing.body(&mut kept));
        assert_eq!(kept, body);
    }
}

/// Once the adapter said what kind of call a result answered, it took the CLI's line saying how a command exited out of
/// its text, and the tags around a tool's own refusal. Before, the text was as the CLI sent it.
#[test]
fn what_went_wrong_is_read_out_of_the_cli_s_words_only_where_the_adapter_hadn_t() {
    let failed = |text: &str, said: Option<Value>| {
        let mut result = json!({"type":"tool_result","tool_use_id":"c","is_error":true,"text":text});
        if let Some(said) = said {
            result["call"] = said;
        }
        result
    };
    let command = || Some(json!({"kind":"command"}));
    let mut said_how = failed("ls: x: No such file", command());
    said_how["exit_code"] = json!(2);

    assert_eq!(output("Bash", &json!({"command":"ls x"}), said_how), printed("ls: x: No such file", Some(2)));
    let as_sent = failed("Exit code 2\nls: x: No such file", None);
    assert_eq!(output("Bash", &json!({"command":"ls x"}), as_sent), printed("ls: x: No such file", Some(2)));
    assert_eq!(
        output("Bash", &json!({"command":"cargo build"}), failed("Command timed out", None)),
        printed("Command timed out", None)
    );
    let refusal = "<tool_use_error>File does not exist.</tool_use_error>";
    assert_eq!(output("Read", &json!({"file_path":"/a"}), failed(refusal, None)), words("File does not exist."));
    let read = Some(json!({"kind":"read"}));
    assert_eq!(output("Read", &json!({"file_path":"/a"}), failed("Exit code 2", read)), words("Exit code 2"), "said");
}

/// As when the CLI answered a call from before it was started again.
#[test]
fn a_result_whose_call_isnt_known_is_a_command_s_where_it_was_one_and_words_otherwise() {
    let alone = |result: Value| typed(vec![message("agent_user", &[result])]);
    let mut exited =
        json!({"type":"tool_result","tool_use_id":"x","is_error":true,"text":"no","call":{"kind":"command"}});
    exited["exit_code"] = json!(1);
    let as_sent = json!({"type":"tool_result","tool_use_id":"x","is_error":true,"text":"Exit code 4\nbad"});
    let unknown =
        json!({"type":"tool_result","tool_use_id":"x","is_error":false,"text":"a.rs","call":{"kind":"unknown"}});

    let outputs: Vec<CallOutput> = [exited, as_sent, unknown]
        .into_iter()
        .flat_map(alone)
        .map(|block| {
            let ContentBlock::CallResult { output, .. } = block else { panic!("{block:?}") };
            output
        })
        .collect();

    assert_eq!(outputs, [printed("no", Some(1)), printed("bad", Some(4)), words("a.rs")]);
}

#[test]
fn what_an_edit_changed_and_what_a_subagent_sent_back_are_their_outputs() {
    let change = FileChange::Created;
    let mut wrote = tool_result("c", "edit", "The file /a.rs has been updated.");
    wrote["change"] = serde_json::to_value(&change).unwrap();
    let edit = json!({"file_path":"/a.rs","old_string":"a","new_string":"b"});
    let file = FileOutcome { path: "/a.rs".into(), change: Some(change), moved_to: None, diagnostics: Vec::new() };
    assert_eq!(output("Edit", &edit, wrote), CallOutput::Changed { files: vec![file] });

    let agent = json!({"prompt":"Look","run_in_background":true});
    let launched = tool_result("c", "subagent", "Async agent launched successfully. agentId: a1");
    assert_eq!(output("Agent", &agent, launched), CallOutput::Backgrounded { task: None });
    let report = tool_result("c", "subagent", "Found it.");
    assert_eq!(output("Agent", &agent, report), CallOutput::Report { report: "Found it.".into() });

    let encrypted = json!({"type":"tool_result","tool_use_id":"c","is_error":false,"text":"","encrypted":true});
    assert_eq!(output("advisor", &json!({}), encrypted), CallOutput::Encrypted);
}

/// The adapter said what kind of call a request to use a tool asked to make, and before that, nothing of what one
/// asked, which its tool and input said.
#[test]
fn a_request_for_approval_asks_to_make_its_call_typed() {
    let request = |tool: &str, input: Value, asking: Option<Value>| {
        let mut body = json!({"type":"approval_requested","approval_id":"a","tool_name":tool,"tool_use_id":"c",
            "input":input,"description":null});
        if let Some(asking) = asking {
            body["asking"] = asking;
        }
        body
    };
    let asking = |mut body: Value| {
        assert!(Typing::default().body(&mut body), "{body}");
        serde_json::from_value::<Asking>(body["asking"].take()).unwrap()
    };
    let said_kind = Some(json!({"kind":"tool_use","call":{"kind":"command"}}));

    assert_eq!(
        asking(request("Bash", json!({"command":"ls"}), said_kind)),
        Asking::ToolUse { typed: Some(bash("ls")) }
    );
    assert_eq!(asking(request("Bash", json!({"command":"ls"}), None)), Asking::ToolUse { typed: Some(bash("ls")) });
    let questions = json!({"questions":[{"question":"Red or blue?","options":[{"label":"Red"}]}]});
    let Asking::Questions { questions } = asking(request("AskUserQuestion", questions, None)) else { panic!() };
    assert_eq!(questions.iter().map(|q: &Question| q.text.as_str()).collect::<Vec<_>>(), ["Red or blue?"]);
    assert_eq!(asking(request("ExitPlanMode", json!({"plan":"1."}), None)), Asking::Plan { plan: "1.".into() });

    let typed = serde_json::to_value(Asking::ToolUse { typed: Some(bash("ls")) }).unwrap();
    let mut own = request("Bash", json!({"command":"ls"}), Some(typed));
    let mut codex = request("", Value::Null, None);
    codex["request"] = json!({"asks":{"kind":"unknown"}});
    for body in [&mut own, &mut codex] {
        let kept = body.clone();
        assert!(!Typing::default().body(body), "already typed, or in a form of its own");
        assert_eq!(*body, kept);
    }
}

/// A worker of a build from before the adapter typed its calls sends them untyped, and one of a build since sends what
/// reads as it did.
#[test]
fn a_worker_s_frame_reads_with_its_agent_s_calls_typed() {
    let frame = |body: Value| {
        json!({"frame":"agent_event","session_id":"s","run":1,"body":body,"raw":r#"{"type":"tool_use"}"#}).to_string()
    };
    let mut typing = Typing::default();
    let read = |typing: &mut Typing, body: Value| {
        let WorkerFrame::AgentEvent { body, .. } = typing.frame(frame(body).as_bytes()).unwrap() else { panic!() };
        serde_json::to_value(body).unwrap()["blocks"].take()
    };

    let made = read(&mut typing, message("assistant", &[tool_use("c", "Bash", &json!({"command":"ls"}))]));
    let came = read(&mut typing, message("agent_user", &[tool_result("c", "command", "a.rs")]));
    let since = message("assistant", &[json!({"type":"text","text":r#""type":"tool_use""#})]);

    let typed = |block: ContentBlock| serde_json::to_value([block]).unwrap();
    assert_eq!(made, typed(ContentBlock::Call { id: "c".into(), call: bash("ls") }));
    let result = ContentBlock::CallResult {
        tool_use_id: "c".into(),
        output: printed("a.rs", None),
        is_error: false,
        denied: None,
        images: Vec::new(),
    };
    assert_eq!(came, typed(result), "read against its call, from an earlier frame");
    assert_eq!(read(&mut typing, since.clone()), since["blocks"]);
}

#[test]
fn a_subagent_s_record_of_what_it_did_lately_is_typed() {
    let recent = json!({
        "calls": [{
            "before": {"text":"Editing.","cut":false},
            "call": {"id":"e","name":"Edit","input":{"file_path":"/a.rs","old_string":"a","new_string":"b"}},
            "result": {"is_error":false,"text":"Updated.","cut":false,"images":[]},
        }, {
            "call": {"id":"b","name":"Bash","input":{"command":"make"}},
        }],
        "since": {"text":"Building.","cut":true},
    });

    let typed = lately(&recent.to_string()).unwrap();

    let edit = Call::Edit(Edit {
        path: "/a.rs".into(),
        edits: vec![Replace { old: "a".into(), new: "b".into(), replaces: Replaces::One }],
    });
    let changed = FileOutcome { path: "/a.rs".into(), change: None, moved_to: None, diagnostics: Vec::new() };
    let result = LatelyResult {
        output: CallOutput::Changed { files: vec![changed] },
        is_error: false,
        cut: false,
        images: Vec::new(),
        denied: None,
    };
    let said = |text: &str, cut| Some(Said { text: text.into(), cut });
    let edited = LatelyCall { before: said("Editing.", false), id: "e".into(), call: edit, result: Some(result) };
    let building = LatelyCall { before: None, id: "b".into(), call: bash("make"), result: None };
    assert_eq!(typed, Lately { calls: vec![edited, building], since: said("Building.", true) });
    assert_eq!(lately(r#"{"calls":[],"since":{"text":"Hi.","cut":false}}"#), None, "no call to type");
}
