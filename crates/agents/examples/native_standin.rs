//! A scripted app-server used by adapter tests on every platform.
#![expect(clippy::unwrap_used, reason = "a test program whose failure must fail its caller")]

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead as _, Write as _};

fn main() {
    if std::env::args().any(|arg| arg == "--descendant") {
        hold();
    }
    let rules: Vec<Value> = serde_json::from_slice(&std::fs::read("native-script.json").unwrap()).unwrap();
    let mut calls: HashMap<String, u64> = HashMap::new();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        say(&json!({"method":"test/request","params":request}));
        let method = request["method"].as_str().unwrap_or_default();
        let count = calls.entry(method.to_owned()).or_default();
        *count += 1;
        if let Some(rule) =
            rules.iter().find(|rule| rule["on"] == method && rule["at"].as_u64().is_none_or(|at| at == *count))
        {
            for action in rule["actions"].as_array().unwrap() {
                act(action, &request);
            }
        } else {
            match method {
                "initialize" => say(&json!({"id":request["id"],"result":{}})),
                "thread/start" => {
                    say(&json!({"id":request["id"],"result":{"thread":{"id":"thread"},"model":"gpt-6-luna"}}));
                }
                _ => {}
            }
        }
    }
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
            if output.get("method").is_none() {
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
