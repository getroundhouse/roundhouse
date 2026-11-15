//! What a launch is told to add to the agent's system prompt and its subagents', in the files it's given, which the real
//! CLI reads before it does anything else.

use std::path::Path;

use crate::settings::flag;

/// A flag that gives a file whose text is added to a system prompt.
struct Added {
    flag: &'static str,
    /// What the real CLI calls it when its file isn't there.
    called: &'static str,
    /// Where a copy of what the file said is kept in `$FAKE_CLAUDE_STATE`.
    kept: &'static str,
}

const ADDED: [Added; 2] = [
    Added { flag: "--append-system-prompt-file", called: "Append system prompt", kept: "last-system-prompt" },
    Added {
        flag: "--append-subagent-system-prompt-file",
        called: "Append subagent system prompt",
        kept: "last-subagent-system-prompt",
    },
];

/// Keeps what each file this launch was given said, so a test can see it, since the adapter removes the file once the
/// agent exits. A launch given none keeps nothing. Exits as the real CLI does when a file isn't there.
pub(crate) fn keep(args: &[String], state: &Path) {
    for Added { flag: name, called, kept } in ADDED {
        let kept = state.join(kept);
        let Some(file) = flag(args, name) else {
            let _ = std::fs::remove_file(kept);
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&file) else {
            eprintln!("Error: {called} file not found: {file}");
            std::process::exit(1);
        };
        std::fs::write(kept, text).unwrap();
    }
}
