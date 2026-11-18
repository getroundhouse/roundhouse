//! A scripted app-server used by adapter tests on every platform.
#![expect(clippy::unwrap_used, reason = "a test program whose failure must fail its caller")]

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead as _, Write as _};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--descendant") {
        hold();
    }
    if args.first().map(String::as_str) == Some("models") {
        run_models();
    }
    let (script_bytes, root) = if let Ok(bytes) = std::fs::read("native-script.json") {
        (bytes, Path::new("."))
    } else {
        (std::fs::read("../native-script.json").unwrap(), Path::new(".."))
    };
    let mut calls = prior_calls(root);
    record_line(root, &json!({ "launch_args": args }));
    let rules: Vec<Value> = serde_json::from_slice(&script_bytes).unwrap();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        record_line(root, &request);
        say(&json!({"method":"test/request","params":request}));
        let method = request["method"].as_str().unwrap_or_default();
        let count = calls.entry(method.to_owned()).or_default();
        *count += 1;
        if let Some(rule) = rules.iter().find(|rule| matches(rule, method, *count, &request["params"])) {
            for action in rule["actions"].as_array().unwrap() {
                act(action, &request);
            }
        } else {
            match method {
                "initialize" | "session/close" | "session/set_mode" | "session/set_config_option" => {
                    say(&json!({"id":request["id"],"result":{}}));
                }
                "thread/start" => {
                    say(&json!({"id":request["id"],"result":{"thread":{"id":"thread"},"model":"gpt-6-luna"}}));
                }
                "session/new" | "session/resume" => {
                    let session_id = request["params"]["sessionId"].as_str().unwrap_or("session-1");
                    say(&json!({
                        "id": request["id"],
                        "result": {
                            "sessionId": session_id,
                            "configOptions": [{
                                "id": "thought_level",
                                "currentValue": "medium",
                                "options": [
                                    {"value": "low", "name": "Low"},
                                    {"value": "medium", "name": "Medium"},
                                    {"value": "high", "name": "High"},
                                    {"value": "xhigh", "name": "Extra High"}
                                ]
                            }]
                        }
                    }));
                }
                _ => {}
            }
        }
    }
}

fn matches(rule: &Value, method: &str, count: u64, params: &Value) -> bool {
    rule["on"] == method
        && rule["at"].as_u64().is_none_or(|at| at == count)
        && rule.get("params").is_none_or(|expected| expected == params)
}

fn prior_calls(root: &Path) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    let Ok(existing) = std::fs::read_to_string(root.join("native-requests.jsonl")) else { return counts };
    for line in existing.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line)
            && let Some(method) = value["method"].as_str()
        {
            *counts.entry(method.to_owned()).or_default() += 1;
        }
    }
    counts
}

fn run_models() -> ! {
    let (bytes, root) = if let Ok(bytes) = std::fs::read("native-models.json") {
        (bytes, Path::new("."))
    } else {
        (std::fs::read("../native-models.json").unwrap(), Path::new(".."))
    };
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    if spec["descendant"].as_bool() == Some(true) {
        #[expect(
            clippy::zombie_processes,
            reason = "the test kills this process and its deliberately live descendant together"
        )]
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--descendant")
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::fs::write(root.join("descendant"), child.id().to_string()).unwrap();
    }
    if spec["hold"].as_bool() == Some(true) {
        std::fs::write(root.join("models-ready"), "ready").unwrap();
        hold();
    }
    if let Some(stdout) = spec["stdout"].as_str() {
        print!("{stdout}");
        std::io::stdout().flush().unwrap();
    }
    if let Some(stderr) = spec["stderr"].as_str() {
        eprint!("{stderr}");
        std::io::stderr().flush().unwrap();
    }
    std::process::exit(i32::try_from(spec["exit"].as_i64().unwrap_or(0)).unwrap());
}

fn record_line(root: &Path, value: &Value) {
    let mut file =
        std::fs::OpenOptions::new().create(true).append(true).open(root.join("native-requests.jsonl")).unwrap();
    writeln!(file, "{value}").unwrap();
}

fn act(action: &Value, request: &Value) {
    match action.as_str() {
        Some("descendant") => {
            #[expect(
                clippy::zombie_processes,
                reason = "the test kills this process and its deliberately live descendant together"
            )]
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--descendant")
                .stdin(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::fs::write("descendant", child.id().to_string()).unwrap();
        }
        Some("account-ready") => std::fs::write("account-ready", "ready").unwrap(),
        Some("close-input") => {
            close_input();
            say(&json!({"method":"test/ready","params":{}}));
            hold();
        }
        Some(other) => panic!("unknown script action {other}"),
        None => {
            let mut output = action.clone();
            if output.get("method").is_none() && output.get("id").is_none() {
                output["id"] = request["id"].clone();
            }
            say(&output);
        }
    }
}

fn say(value: &Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}

fn hold() -> ! {
    loop {
        std::thread::park();
    }
}

#[cfg(unix)]
fn close_input() {
    use std::os::fd::{AsRawFd as _, FromRawFd as _};
    // SAFETY: this test process never accesses stdin again: the caller parks until it is killed.
    drop(unsafe { std::fs::File::from_raw_fd(std::io::stdin().as_raw_fd()) });
}

#[cfg(windows)]
fn close_input() {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
    // SAFETY: this test process never accesses stdin again: the caller parks until it is killed.
    drop(unsafe { std::fs::File::from_raw_handle(std::io::stdin().as_raw_handle()) });
}
