//! The one-off call that titles a prompt. It reads the prompt as one `user` line, as a session's are read, and answers
//! with what `$FAKE_CLAUDE_STATE/title` holds, as the real CLI's `result` line, between lines like the ones the real
//! CLI prints around it. It fails if there's no such file, so a test that isn't about titles keeps the prompt's first
//! line. It keeps the prompt's words in `title-prompt`, and what came with them in `title-images`, as `attached` says
//! it. A prompt can carry its own title instead, as the rest of the line after `titled:`, for tests that share a state
//! directory, where a file would title every test's prompts.
//!
//! While `title-held` exists it waits, so a test can act before the title arrives. Meanwhile it runs a process of
//! its own, as a hook would, and writes its PID and that one's to `title-pids`, so a test can tell that stopping the
//! call reaches both.

use std::io::Read as _;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

pub(crate) fn answer(state: &Path) {
    let mut line = String::new();
    std::io::stdin().read_to_string(&mut line).unwrap();
    let message: Value = serde_json::from_str(line.trim()).unwrap();
    let prompt = crate::prompt::read_prompt(&message["message"]["content"]);
    std::fs::write(state.join("title-prompt"), &prompt).unwrap();
    std::fs::write(state.join("title-images"), crate::prompt::attached()).unwrap();
    if state.join("title-held").exists() {
        let mut started = Command::new("sleep").arg("300").spawn().unwrap();
        std::fs::write(state.join("title-pids"), format!("{} {}", std::process::id(), started.id())).unwrap();
        while state.join("title-held").exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        started.kill().unwrap();
        started.wait().unwrap();
    }
    let carried = prompt.split_once("titled:").and_then(|(_, title)| title.lines().next()).map(str::to_string);
    let Some(title) = carried.or_else(|| std::fs::read_to_string(state.join("title")).ok()) else {
        eprintln!("Error: no title to give");
        std::process::exit(1);
    };
    let session_id = "00000000-0000-0000-0000-000000000000";
    println!("{}", json!({"type": "system", "subtype": "init", "session_id": session_id}));
    let result = json!({
        "type": "result", "subtype": "success", "is_error": false, "result": title, "num_turns": 1,
        "session_id": session_id,
    });
    println!("{result}");
    println!("{}", json!({"type": "system", "subtype": "session_state_changed", "state": "idle"}));
}
