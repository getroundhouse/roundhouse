//! The commands that ask for an answer: tools, plans and questions.

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

use serde_json::{Value, json};

use crate::turn::{Turn, state_event};
use crate::{BEFORE_PLAN, Input, Prompt, current_mode, emit, set_mode};

/// The result reads like the real CLI's, which goes by whether it was given answers.
pub(crate) fn ask_questions(
    turn: &mut Turn,
    spec: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let questions: Vec<Value> = spec
        .split('|')
        .map(|one| {
            let (text, options) = one.split_once('=').unwrap();
            let (multi, text) = text.strip_prefix('*').map_or((false, text), |rest| (true, rest));
            let options: Vec<Value> =
                options.split('/').map(|o| json!({"label":o,"description":format!("Choose {o}")})).collect();
            json!({"question":text,"header":"Question","options":options,"multiSelect":multi})
        })
        .collect();
    let tool_use_id = turn.named_tool_use("AskUserQuestion", json!({}));
    let request_id = format!("req_{tool_use_id}");
    emit(
        turn.out,
        json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
        "tool_name":"AskUserQuestion","input":{"questions":questions},"tool_use_id":tool_use_id,
        "permission_suggestions":[]}}),
    );
    state_event(turn.config, turn.out, "requires_action");
    loop {
        match rx.recv() {
            Ok(Input::Approval(response)) => {
                state_event(turn.config, turn.out, "running");
                if response["response"]["behavior"] == "deny" {
                    let message = response["response"]["message"].as_str().unwrap_or("denied");
                    turn.tool_result(&tool_use_id, message, true);
                    return false;
                }
                let answers = response["response"]["updatedInput"]["answers"].as_object();
                let text = match answers {
                    Some(answers) if !answers.is_empty() => {
                        let said: Vec<String> =
                            answers.iter().map(|(q, a)| format!("\"{q}\"={}", a.as_str().unwrap())).collect();
                        format!("Your questions have been answered: {}.", said.join(", "))
                    }
                    _ => "The user did not answer the questions.".to_string(),
                };
                turn.tool_result(&tool_use_id, &text, false);
                return false;
            }
            Ok(Input::Interrupt) => {
                emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                turn.tool_result(&tool_use_id, "cancelled", true);
                return true;
            }
            Ok(Input::Prompt(prompt) | Input::Resume(prompt)) => queued.push_back(prompt),
            Ok(Input::Eof) | Err(_) => {
                *eof = true;
                return true;
            }
        }
    }
}

/// Approving a plan leaves plan mode for the mode it was entered from, and the CLI says so itself.
pub(crate) fn present_plan(
    turn: &mut Turn,
    plan: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.named_tool_use("ExitPlanMode", json!({}));
    let request_id = format!("req_{tool_use_id}");
    emit(
        turn.out,
        json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
        "tool_name":"ExitPlanMode","input":{"plan":plan,"planFilePath":"/tmp/fake-plan.md"},
        "tool_use_id":tool_use_id,"permission_suggestions":[]}}),
    );
    state_event(turn.config, turn.out, "requires_action");
    loop {
        match rx.recv() {
            Ok(Input::Approval(response)) => {
                state_event(turn.config, turn.out, "running");
                if response["response"]["behavior"] == "deny" {
                    let message = response["response"]["message"].as_str().unwrap_or("denied");
                    turn.tool_result(&tool_use_id, message, true);
                    return false;
                }
                let back_to = BEFORE_PLAN.lock().clone().unwrap_or_else(|| "default".to_string());
                set_mode(turn.out, &back_to);
                turn.tool_result(&tool_use_id, "User has approved your plan.", false);
                return false;
            }
            Ok(Input::Interrupt) => {
                emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                turn.tool_result(&tool_use_id, "cancelled", true);
                return true;
            }
            Ok(Input::Prompt(prompt) | Input::Resume(prompt)) => queued.push_back(prompt),
            Ok(Input::Eof) | Err(_) => {
                *eof = true;
                return true;
            }
        }
    }
}

pub(crate) fn run_tool(
    turn: &mut Turn,
    command: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.tool_use(command);
    if current_mode() != "bypassPermissions" {
        let request_id = format!("req_{tool_use_id}");
        emit(
            turn.out,
            json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
            "tool_name":"Bash","input":{"command":command},"description":command,"tool_use_id":tool_use_id,
            "permission_suggestions":[]}}),
        );
        state_event(turn.config, turn.out, "requires_action");
        loop {
            match rx.recv() {
                Ok(Input::Approval(response)) => {
                    state_event(turn.config, turn.out, "running");
                    assert_eq!(response["request_id"], request_id.as_str());
                    if response["response"]["behavior"] == "deny" {
                        let message = response["response"]["message"].as_str().unwrap_or("denied");
                        turn.tool_result(&tool_use_id, message, true);
                        return false;
                    }
                    // The CLI runs what it's given back, which is the input as approved.
                    let approved =
                        response["response"]["updatedInput"]["command"].as_str().unwrap_or(command).to_string();
                    return execute(turn, &tool_use_id, &approved);
                }
                Ok(Input::Interrupt) => {
                    emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                    turn.tool_result(&tool_use_id, "cancelled", true);
                    return true;
                }
                Ok(Input::Prompt(prompt) | Input::Resume(prompt)) => queued.push_back(prompt),
                Ok(Input::Eof) | Err(_) => {
                    *eof = true;
                    return true;
                }
            }
        }
    }
    execute(turn, &tool_use_id, command)
}

/// Changes each of the lines of `notes.txt` that `size` says, `N` or `N:WIDTH`, each at least `WIDTH` characters long.
pub(crate) fn edit_file(turn: &mut Turn, size: &str) -> bool {
    let (lines, width) = size.split_once(':').unwrap_or((size, "0"));
    let (lines, width): (u32, usize) = (lines.parse().unwrap(), width.parse().unwrap());
    let line = |word: &str, n: u32| {
        let mut line = format!("{word} line {n}");
        while line.len() < width {
            line.push_str(" and so on");
        }
        line
    };
    let text = |word: &str| (1..=lines).map(|n| line(word, n)).collect::<Vec<_>>().join("\n");
    let path = std::env::current_dir().unwrap().join("notes.txt");
    let input = json!({"file_path":path,"old_string":text("old"),"new_string":text("new")});
    let tool_use_id = turn.named_tool_use("Edit", input);
    let said = format!("The file {} has been updated successfully.", path.display());
    turn.tool_result(&tool_use_id, &said, false);
    false
}

pub(crate) fn execute(turn: &mut Turn, tool_use_id: &str, command: &str) -> bool {
    let output = std::process::Command::new("sh").args(["-c", command]).output().unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let worked = output.status.success();
    // As the CLI does, a command that worked and printed nothing says so in a sentence of its own.
    let text = if worked && stdout.is_empty() && stderr.is_empty() { NOTHING_PRINTED } else { &stdout };
    // With the record of what it printed that the CLI puts beside it.
    let mut line = turn.result_line(tool_use_id, text, !worked);
    line["tool_use_result"] = json!({ "stdout": stdout, "stderr": stderr, "interrupted": false });
    emit(turn.out, line);
    false
}

/// What the CLI sends as a command's result when the command printed nothing.
const NOTHING_PRINTED: &str = "(Bash completed with no output)";
