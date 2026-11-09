//! The one-off call that titles a prompt. It answers with what `$FAKE_CLAUDE_STATE/title` holds, shaped like the
//! real CLI's single JSON result, and fails if there's no such file, so a test that isn't about titles keeps the
//! prompt's first line. It keeps the prompt it was given in `title-prompt`.
//!
//! While `title-held` exists it waits, so a test can act before the title arrives. Meanwhile it runs a process of
//! its own, as a hook would, and writes its PID and that one's to `title-pids`, so a test can tell that stopping the
//! call reaches both.

use std::io::Read as _;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::json;

pub(crate) fn answer(state: &Path) {
    let mut prompt = String::new();
    std::io::stdin().read_to_string(&mut prompt).unwrap();
    std::fs::write(state.join("title-prompt"), &prompt).unwrap();
    if state.join("title-held").exists() {
        let mut started = Command::new("sleep").arg("300").spawn().unwrap();
        std::fs::write(state.join("title-pids"), format!("{} {}", std::process::id(), started.id())).unwrap();
        while state.join("title-held").exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        started.kill().unwrap();
        started.wait().unwrap();
    }
    let Ok(title) = std::fs::read_to_string(state.join("title")) else {
        eprintln!("Error: no title to give");
        std::process::exit(1);
    };
    let result = json!({
        "type": "result", "subtype": "success", "is_error": false, "result": title, "num_turns": 1,
        "session_id": "00000000-0000-0000-0000-000000000000",
    });
    println!("{result}");
}
