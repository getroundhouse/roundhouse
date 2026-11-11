//! Chats that read, write, edit and search files, for the screens about those calls. The same made-up signalling
//! crate as `tool_chats.rs`.

#![cfg(test)]

use protocol::attachment::Attachment;
use protocol::event::{EventBody, FileChange, Hunk, HunkLine, LineMark};
use serde_json::json;

use super::events::{
    finished_turn, replied, taken, text, tool_changed, tool_failed, tool_images, tool_result, tool_use, user_prompt,
};
use super::tool_chats::ASK;
const SIGNAL: &str = "/workspaces/aspen/crates/signals/src/signal.rs";

const SIGNAL_RS: &str = "//! What a signal on the line shows, and what it tells a train to do.

/// What a signal on the line tells a train to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Clear,
    Caution,
    Stop,
}

impl Aspect {
    /// The next aspect down the line, as a train passes.
    pub fn next(self) -> Self {
        match self {
            Self::Clear => Self::Caution,
            Self::Caution => Self::Stop,
            Self::Stop => Self::Stop,
        }
    }

    /// What the signal shows, for a log line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => \"clear\",
            Self::Caution => \"caution\",
            Self::Stop => \"stop\",
        }
    }
}";

const SIGNAL_TEST_RS: &str = "use crate::signal::Aspect;

#[test]
fn stop_may_not_pass() {
    assert!(!Aspect::Stop.may_pass(), \"a train waits at a stop signal until the block ahead of it is clear again\");
}

#[test]
fn clear_and_caution_may_pass() {
    assert!(Aspect::Clear.may_pass());
    assert!(Aspect::Caution.may_pass());
}";

/// Each kind of file call closed, and an edit opened, with the file's own line numbers.
pub(crate) fn files() -> Vec<EventBody> {
    let reply = "The signal can say whether a train may pass, with a test.";
    let mut events = vec![
        user_prompt(ASK),
        taken(1),
        text("msg-1", "I'll read the signal first."),
        tool_use("msg-2", "toolu-1", "Read", json!({ "file_path": SIGNAL })),
        tool_result("toolu-1", &numbered(SIGNAL_RS.lines(), 1)),
        text("msg-3", "A method on `Aspect` is enough, since only the train matches on it."),
    ];
    events.extend(may_pass_edit("msg-4", "toolu-2"));
    events.extend([
        tool_use(
            "msg-5",
            "toolu-3",
            "Write",
            json!({ "file_path": "/workspaces/aspen/crates/signals/src/signal_test.rs", "content": SIGNAL_TEST_RS }),
        ),
        tool_changed("toolu-3", "File created successfully.", FileChange::Created),
        tool_use(
            "msg-6",
            "toolu-4",
            "Edit",
            json!({
                "file_path": "/workspaces/aspen/crates/signals/src/train.rs",
                "old_string": "if signal == Aspect::Stop {",
                "new_string": "if !signal.may_pass() {",
            }),
        ),
        tool_failed("toolu-4", "<tool_use_error>String to replace not found in file.</tool_use_error>"),
    ]);
    events.extend(changelog_write("msg-7", "toolu-5"));
    events.extend([text("msg-8", reply), replied(reply, 52, 0.24)]);
    events
}

/// Adding `may_pass` to the signal, and the patch that came back.
fn may_pass_edit(message: &str, id: &str) -> [EventBody; 2] {
    let input = json!({
        "file_path": SIGNAL,
        "old_string": "            Self::Stop => Self::Stop,\n        }\n    }\n\n    /// What the signal shows",
        "new_string": "            Self::Stop => Self::Stop,\n        }\n    }\n\n    /// Whether a train may pass the signal without stopping.\n    pub fn may_pass(self) -> bool {\n        !matches!(self, Self::Stop)\n    }\n\n    /// What the signal shows",
    });
    [
        tool_use(message, id, "Edit", input),
        tool_changed(id, "The file has been updated successfully.", may_pass_change()),
    ]
}

/// Writing over a changelog that was there, and the patch that came back.
fn changelog_write(message: &str, id: &str) -> [EventBody; 2] {
    let input = json!({
        "file_path": "/workspaces/aspen/CHANGELOG.md",
        "content": "# Changelog\n\n## Unreleased\n\n- Signals say whether a train may pass.\n",
    });
    let lines =
        [" # Changelog", " ", "+## Unreleased", "+", "+- Signals say whether a train may pass.", "+", " ## 0.1.0"];
    let change = FileChange::Changed { hunks: vec![hunk(1, 1, &lines)] };
    [tool_use(message, id, "Write", input), tool_changed(id, "The file has been updated successfully.", change)]
}

/// A read and a new file, opened to their lines.
pub(crate) fn file_contents() -> Vec<EventBody> {
    vec![
        user_prompt(ASK),
        taken(1),
        tool_use("msg-1", "toolu-1", "Read", json!({ "file_path": SIGNAL, "offset": 11, "limit": 10 })),
        tool_result("toolu-1", &numbered(SIGNAL_RS.lines().skip(10).take(10), 11)),
        tool_use(
            "msg-2",
            "toolu-2",
            "Write",
            json!({
                "file_path": "/workspaces/aspen/crates/signals/src/signal_test.rs",
                "content": SIGNAL_TEST_RS,
            }),
        ),
        tool_changed("toolu-2", "File created successfully.", FileChange::Created),
        finished_turn(12, 0.06),
    ]
}

/// The agent reading a screenshot, which the hub keeps as `image`.
pub(crate) fn read_image(image: Attachment) -> Vec<EventBody> {
    let shot = "/workspaces/aspen/.context/attachments/narrow-sidebar.png";
    vec![
        user_prompt(
            "Is the badge still over the name? I saved how it looks in .context/attachments/narrow-sidebar.png",
        ),
        taken(1),
        tool_use("msg-1", "toolu-1", "Read", json!({ "file_path": shot })),
        tool_images("toolu-1", vec![image]),
        text("msg-2", "It is: the badge still sits over the end of the second workspace's name."),
        finished_turn(9, 0.04),
    ]
}

/// Searches: by what's in files, listing files and then lines, and by name.
pub(crate) fn searches() -> Vec<EventBody> {
    let reply = "Only the train and the depot check for it, and the signal itself.";
    let found = "/workspaces/aspen/crates/signals/src/signal.rs:16:            Self::Caution => Self::Stop,\n\
                 /workspaces/aspen/crates/signals/src/signal.rs:17:            Self::Stop => Self::Stop,\n\
                 /workspaces/aspen/crates/signals/src/train.rs:53:        if signal == Aspect::Stop {\n\
                 /workspaces/aspen/crates/signals/src/depot.rs:19:    if aspect == Aspect::Stop { return None; }";
    vec![
        user_prompt("Where does anything check for a stop signal?"),
        taken(1),
        tool_use("msg-1", "toolu-1", "Grep", json!({ "pattern": "Stop\\b", "output_mode": "content", "-n": true })),
        tool_result("toolu-1", found),
        tool_use("msg-2", "toolu-2", "Grep", json!({ "pattern": "Aspect::", "path": "/workspaces/aspen/crates" })),
        tool_result(
            "toolu-2",
            "Found 3 files\n/workspaces/aspen/crates/signals/src/signal.rs\n/workspaces/aspen/crates/signals/src/train.rs\n/workspaces/aspen/crates/signals/src/depot.rs",
        ),
        tool_use("msg-3", "toolu-3", "Glob", json!({ "pattern": "**/*_test.rs" })),
        tool_result(
            "toolu-3",
            "/workspaces/aspen/crates/signals/src/signal_test.rs\n/workspaces/aspen/crates/signals/src/depot_test.rs",
        ),
        text("msg-4", reply),
        replied(reply, 9, 0.03),
    ]
}

/// An edit and a search that failed, whose errors run past their first line.
pub(crate) fn failures() -> Vec<EventBody> {
    let edit = json!({
        "file_path": "/workspaces/aspen/crates/signals/src/depot.rs",
        "old_string": "    if aspect == Aspect::Stop {\n        return None;\n    }",
        "new_string": "    if !aspect.may_pass() {\n        return None;\n    }",
    });
    vec![
        user_prompt("Use may_pass in the depot too."),
        taken(1),
        tool_use("msg-1", "toolu-1", "Edit", edit),
        tool_failed(
            "toolu-1",
            "<tool_use_error>String to replace not found in file.\nString:     if aspect == Aspect::Stop {\n        return None;\n    }</tool_use_error>",
        ),
        text("msg-2", "The depot's check is on one line. I'll find every check like it."),
        tool_use("msg-3", "toolu-2", "Grep", json!({ "pattern": "Aspect::Stop)", "output_mode": "content" })),
        tool_failed(
            "toolu-2",
            "<tool_use_error>regex parse error:\n    Aspect::Stop)\n                ^\nerror: unopened group</tool_use_error>",
        ),
    ]
}

fn may_pass_change() -> FileChange {
    let lines = [
        "             Self::Stop => Self::Stop,",
        "         }",
        "     }",
        " ",
        "+    /// Whether a train may pass the signal without stopping.",
        "+    pub fn may_pass(self) -> bool {",
        "+        !matches!(self, Self::Stop)",
        "+    }",
        "+",
        "     /// What the signal shows, for a log line.",
        "     pub fn label(self) -> &'static str {",
    ];
    FileChange::Changed { hunks: vec![hunk(17, 17, &lines)] }
}

fn hunk(old_start: u32, new_start: u32, lines: &[&str]) -> Hunk {
    let line = |line: &&str| {
        let (mark, text) = line.split_at(1);
        let mark = match mark {
            "-" => LineMark::Removed,
            "+" => LineMark::Added,
            _ => LineMark::Same,
        };
        HunkLine { mark, text: text.to_string() }
    };
    Hunk { old_start, new_start, lines: lines.iter().map(line).collect() }
}

/// Lines as Claude reads them back: each after its number and a tab.
fn numbered<'a>(lines: impl Iterator<Item = &'a str>, first: u32) -> String {
    (first..).zip(lines).map(|(n, line)| format!("{n:>6}\t{line}")).collect::<Vec<_>>().join("\n")
}
