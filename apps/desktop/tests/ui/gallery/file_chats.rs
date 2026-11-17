//! Chats that read, write, edit and search files, for the screens about those calls. The same made-up signalling
//! crate as `tool_chats.rs`.

#![cfg(test)]

use protocol::attachment::Attachment;
use protocol::capability::calls::call::{FindFiles, Search, Shows};
use protocol::capability::calls::output::{Found, FoundLine, Paths};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{EventBody, FileChange, Hunk, HunkLine, LineMark};
use protocol::model::Seq;

use super::tool_chats::ASK;
use client_core::fake_hub::events::{
    changed, edit, failed, finished_turn, images, read, read_back, read_lines, replied, taken, text, typed_call,
    typed_result, user_prompt, write,
};
const SIGNAL: &str = "/workspaces/aspen/crates/signals/src/signal.rs";
const SIGNAL_TEST: &str = "/workspaces/aspen/crates/signals/src/signal_test.rs";
const CHANGELOG: &str = "/workspaces/aspen/CHANGELOG.md";

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
        taken(Seq::new(1)),
        text("msg-1", "I'll read the signal first."),
        read("msg-2", "toolu-1", SIGNAL),
        read_back("toolu-1", 1, SIGNAL_RS),
        text("msg-3", "A method on `Aspect` is enough, since only the train matches on it."),
    ];
    events.extend(may_pass_edit("msg-4", "toolu-2"));
    events.extend([
        write("msg-5", "toolu-3", SIGNAL_TEST, SIGNAL_TEST_RS),
        changed("toolu-3", SIGNAL_TEST, FileChange::Created),
        edit(
            "msg-6",
            "toolu-4",
            "/workspaces/aspen/crates/signals/src/train.rs",
            "if signal == Aspect::Stop {",
            "if !signal.may_pass() {",
        ),
        failed("toolu-4", "String to replace not found in file."),
    ]);
    events.extend(changelog_write("msg-7", "toolu-5"));
    events.extend([text("msg-8", reply), replied(reply, 52, 0.24)]);
    events
}

/// Adding `may_pass` to the signal, and the patch that came back.
fn may_pass_edit(message: &str, id: &str) -> [EventBody; 2] {
    let old = "            Self::Stop => Self::Stop,\n        }\n    }\n\n    /// What the signal shows";
    let new = "            Self::Stop => Self::Stop,\n        }\n    }\n\n    /// Whether a train may pass the signal without stopping.\n    pub fn may_pass(self) -> bool {\n        !matches!(self, Self::Stop)\n    }\n\n    /// What the signal shows";
    [edit(message, id, SIGNAL, old, new), changed(id, SIGNAL, may_pass_change())]
}

/// Writing over a changelog that was there, and the patch that came back.
fn changelog_write(message: &str, id: &str) -> [EventBody; 2] {
    let content = "# Changelog\n\n## Unreleased\n\n- Signals say whether a train may pass.\n";
    let lines =
        [" # Changelog", " ", "+## Unreleased", "+", "+- Signals say whether a train may pass.", "+", " ## 0.1.0"];
    let change = FileChange::Changed { hunks: vec![hunk(1, 1, &lines)] };
    [write(message, id, CHANGELOG, content), changed(id, CHANGELOG, change)]
}

/// A read and a new file, opened to their lines.
pub(crate) fn file_contents() -> Vec<EventBody> {
    vec![
        user_prompt(ASK),
        taken(Seq::new(1)),
        read_lines("msg-1", "toolu-1", SIGNAL, 11, 10),
        read_back("toolu-1", 11, &SIGNAL_RS.lines().skip(10).take(10).collect::<Vec<_>>().join("\n")),
        write("msg-2", "toolu-2", SIGNAL_TEST, SIGNAL_TEST_RS),
        changed("toolu-2", SIGNAL_TEST, FileChange::Created),
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
        taken(Seq::new(1)),
        read("msg-1", "toolu-1", shot),
        images("toolu-1", vec![image]),
        text("msg-2", "It is: the badge still sits over the end of the second workspace's name."),
        finished_turn(9, 0.04),
    ]
}

/// Searches: by what's in files, listing files and then lines, and by name.
pub(crate) fn searches() -> Vec<EventBody> {
    let reply = "Only the train and the depot check for it, and the signal itself.";
    let at = |path: &str, line, text: &str| FoundLine {
        path: format!("/workspaces/aspen/crates/signals/src/{path}"),
        line,
        text: text.into(),
    };
    let lines = vec![
        at("signal.rs", 16, "            Self::Caution => Self::Stop,"),
        at("signal.rs", 17, "            Self::Stop => Self::Stop,"),
        at("train.rs", 53, "        if signal == Aspect::Stop {"),
        at("depot.rs", 19, "    if aspect == Aspect::Stop { return None; }"),
    ];
    let files = |paths: &[&str]| {
        let paths = paths.iter().map(|path| format!("/workspaces/aspen/crates/signals/src/{path}")).collect();
        CallOutput::Found(Found::Files(Paths { paths, truncated: false }))
    };
    vec![
        user_prompt("Where does anything check for a stop signal?"),
        taken(Seq::new(1)),
        typed_call("msg-1", "toolu-1", search("Stop\\b", None, Shows::Lines)),
        typed_result("toolu-1", CallOutput::Found(Found::Lines { lines, truncated: false }), false),
        typed_call("msg-2", "toolu-2", search("Aspect::", Some("/workspaces/aspen/crates"), Shows::Files)),
        typed_result("toolu-2", files(&["signal.rs", "train.rs", "depot.rs"]), false),
        typed_call("msg-3", "toolu-3", Call::FindFiles(FindFiles { pattern: "**/*_test.rs".into(), path: None })),
        typed_result("toolu-3", files(&["signal_test.rs", "depot_test.rs"]), false),
        text("msg-4", reply),
        replied(reply, 9, 0.03),
    ]
}

/// An edit and a search that failed, whose errors run past their first line.
pub(crate) fn failures() -> Vec<EventBody> {
    vec![
        user_prompt("Use may_pass in the depot too."),
        taken(Seq::new(1)),
        edit(
            "msg-1",
            "toolu-1",
            "/workspaces/aspen/crates/signals/src/depot.rs",
            "    if aspect == Aspect::Stop {\n        return None;\n    }",
            "    if !aspect.may_pass() {\n        return None;\n    }",
        ),
        failed(
            "toolu-1",
            "String to replace not found in file.\nString:     if aspect == Aspect::Stop {\n        return None;\n    }",
        ),
        text("msg-2", "The depot's check is on one line. I'll find every check like it."),
        typed_call("msg-3", "toolu-2", search("Aspect::Stop)", None, Shows::Lines)),
        failed("toolu-2", "regex parse error:\n    Aspect::Stop)\n                ^\nerror: unopened group"),
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

/// Looking for `pattern`, under `path` if it says, showing what `shows` says.
fn search(pattern: &str, path: Option<&str>, shows: Shows) -> Call {
    Call::Search(Search { pattern: pattern.into(), path: path.map(Into::into), files: None, shows })
}
