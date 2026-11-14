//! The Claude stream controls used to verify a login, with synthetic scripted replies on every platform.
#![expect(clippy::unwrap_used, reason = "a test program whose failure must fail its caller")]

use serde_json::{Value, json};
use std::io::{BufRead as _, Write as _};
use std::path::Path;

fn main() {
    let binary = std::env::current_exe().unwrap();
    let dir = binary.parent().unwrap();
    let script: Value = serde_json::from_slice(&std::fs::read(dir.join("login-script.json")).unwrap()).unwrap();
    let mode = script["mode"].as_str().unwrap_or_default();
    if mode == "record-login" && record_login(dir) {
        return;
    }
    let attempt = std::fs::read_to_string(dir.join("attempts")).ok().map_or(0, |n| n.parse::<u32>().unwrap()) + 1;
    std::fs::write(dir.join("attempts"), attempt.to_string()).unwrap();
    if mode == "retry-before" && attempt == 1 {
        mismatch();
    }
    if mode == "startup-output" {
        say(
            &json!({"type":"assistant","message":{"id":"startup","content":[{"type":"text","text":"buffered-startup"}]}}),
        );
    }
    read(dir, &script, mode, attempt);
}

fn read(dir: &Path, script: &Value, mode: &str, attempt: u32) {
    let mut settings = script["settings"].clone();
    let mut received = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("received")).unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        writeln!(received, "{line}").unwrap();
        received.flush().unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        if mode == "hold-start" || (mode == "retry-stop" && attempt == 2) {
            std::fs::write(dir.join("guard-ready"), std::process::id().to_string()).unwrap();
            loop {
                std::thread::park();
            }
        }
        if request["type"] == "user" {
            if mode == "retry-stop" {
                mismatch();
            }
            say(&json!({"type":"system","subtype":"init","session_id":"s"}));
            say(&json!({"type":"result","subtype":"success","is_error":false,"result":"Checked login"}));
            continue;
        }
        let response = match request["request"]["subtype"].as_str() {
            Some("initialize") => {
                json!({"account":{"tokenSource":"CLAUDE_CODE_OAUTH_TOKEN","apiProvider":"firstParty"}})
            }
            Some("apply_flag_settings") if mode == "record-login" => {
                settings = json!({"effective":request["request"]["settings"]});
                Value::Null
            }
            Some("get_settings") => settings.clone(),
            _ => Value::Null,
        };
        say(
            &json!({"type":"control_response","response":{"subtype":"success","request_id":request["request_id"],"response":response}}),
        );
    }
}

/// Records the environment at the worker boundary; account probes do not use stream controls.
fn record_login(dir: &Path) -> bool {
    let args: Vec<_> = std::env::args().collect();
    let what = if args.iter().any(|arg| arg == "auth") {
        "account"
    } else if args.iter().any(|arg| arg == "--session-id" || arg == "--resume") {
        "agent"
    } else {
        "title"
    };
    let env = |name| std::env::var(name).unwrap_or_else(|_| "none".to_string());
    let mut ran = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("ran")).unwrap();
    writeln!(ran, "{what} {} {}", env("CLAUDE_CODE_OAUTH_TOKEN"), env("ANTHROPIC_API_KEY")).unwrap();
    if what == "account" {
        say(&json!({"loggedIn":true,"authMethod":"oauth_token","apiProvider":"firstParty"}));
    }
    what == "account"
}

fn mismatch() -> ! {
    eprintln!("No conversation found");
    std::process::exit(1);
}

fn say(value: &Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
