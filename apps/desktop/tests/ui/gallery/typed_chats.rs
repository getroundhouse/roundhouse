//! Chats whose calls are recorded typed, as an adapter that types its calls records them, for the screens of how each
//! kind is drawn from it. Made up, about the same signalling crate as the chats of Claude's calls, so they read alike.

#![cfg(test)]

use protocol::capability::calls::call::{
    CellEdit, Command, CommandPart, Fetch, FilePatch, FindFiles, NotebookEdit, Patch, PatchChange, PatchHunk, Read,
    Runs, Search, Shows, WebSearch,
};
use protocol::capability::calls::output::{Ended, FileOutcome, Found, Lines, Paths};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{EventBody, FileChange, Hunk, HunkLine, LineMark};
use protocol::fixtures::{command_output, typed_command};
use protocol::model::Seq;

use super::events::{replied, taken, text, typed_call, typed_result, user_prompt};
use super::tool_chats::ASK;

/// What the agent said its command in the background is for.
pub(crate) const BACKGROUND_LINE: &str = "Start the depot simulator";

/// Commands that ended without exiting: one that went on in the background, one that ran past its time, and one that
/// was stopped.
pub(crate) fn commands_that_ended() -> Vec<EventBody> {
    let reply = "The slow tests ran past their time, so I stopped the benchmark too. The simulator is still running.";
    let backgrounded = Ended::Backgrounded { task: "simulator".into() };
    vec![
        user_prompt(ASK),
        taken(Seq::new(1)),
        text("msg-1", "I'll start the depot's simulator, then run the slow signal tests against it."),
        typed_call("msg-2", "call-1", command("cargo run -p depot --bin simulator", BACKGROUND_LINE, Runs::Background)),
        typed_result("call-1", command_output(SIMULATOR, None, backgrounded), false),
        typed_call(
            "msg-3",
            "call-2",
            command("cargo test -p signals --release -- --ignored", "Run the slow signal tests", Runs::Waits),
        ),
        typed_result("call-2", command_output(SLOW_TESTS, None, Ended::TimedOut), true),
        typed_call("msg-4", "call-3", command("cargo bench -p signals may_pass", "Benchmark may_pass", Runs::Waits)),
        typed_result("call-3", command_output(BENCHMARK, None, Ended::Stopped), true),
        text("msg-5", reply),
        replied(reply, 214, 0.38),
    ]
}

/// What the simulator printed before it went on in the background.
const SIMULATOR: &str = "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.38s
     Running `target/debug/simulator`
depot simulator listening on 127.0.0.1:7341";

/// What the slow tests printed before they ran past their time.
const SLOW_TESTS: &str = "    Finished `release` profile [optimized] target(s) in 41.20s
     Running unittests src/lib.rs (target/release/deps/signals-91c3e0d2)

running 14 tests
test depot::caution_holds_for_the_whole_block ... ok";

/// What the benchmark printed before it was stopped.
const BENCHMARK: &str = "Benchmarking may_pass: Warming up for 3.0000 s";

/// Running `command`, which the agent said is for `description`.
fn command(command: &str, description: &str, runs: Runs) -> Call {
    Call::Command(Command { description: Some(description.into()), runs, ..typed_command(command) })
}

/// The line of a patch of two files.
pub(crate) const PATCH_LINE: &str = "Edit 2 files";

const SIGNAL: &str = "/workspaces/aspen/crates/signals/src/signal.rs";
const DEPOT: &str = "/workspaces/aspen/crates/depot/src/lib.rs";

/// What the depot's new file holds.
const DEPOT_RS: &str = "use signals::signal::Aspect;

pub fn may_leave(signal: Aspect) -> bool {
    signal.may_pass()
}";

/// Calls of files: a command that does nothing but read one, drawn as reading it, a read with its lines' numbers, a
/// patch of two files, opened to each file's change, and a notebook's cell changed.
pub(crate) fn files_changed() -> Vec<EventBody> {
    let reply = "Signals now say whether a train may pass, and the depot asks before it lets one out.";
    let reads = vec![CommandPart::Read { path: SIGNAL.into() }];
    let sed = Call::Command(Command { parts: reads, ..typed_command("sed -n 1,9p crates/signals/src/signal.rs") });
    let read = Call::Read(Read { path: SIGNAL.into(), lines: None, pages: None });
    let lines = Lines { first: 28, lines: ["        }", "    }", "}"].map(String::from).to_vec(), total: Some(30) };
    let notebook = NotebookEdit {
        path: "/workspaces/aspen/notebooks/headways.ipynb".into(),
        cell: Some("cell-4".into()),
        source: "plot(headways(trains, may_pass=True))".into(),
        does: CellEdit::Replace,
    };
    vec![
        user_prompt(ASK),
        taken(Seq::new(1)),
        text("msg-1", "I'll read the signal, then add `may_pass` to it and use it in the depot."),
        typed_call("msg-2", "sed-1", sed),
        typed_result("sed-1", command_output(SIGNAL_START, Some(0), Ended::Exited), false),
        typed_call("msg-3", "read-1", read),
        typed_result("read-1", CallOutput::Lines(lines), false),
        typed_call("msg-4", "patch-1", Call::Patch(patch())),
        typed_result(
            "patch-1",
            CallOutput::Changed { files: vec![changed(SIGNAL, true), changed(DEPOT, false)] },
            false,
        ),
        typed_call("msg-5", "cell-1", Call::NotebookEdit(notebook)),
        typed_result("cell-1", CallOutput::Words { text: "Updated cell cell-4".into() }, false),
        text("msg-6", reply),
        replied(reply, 96, 0.21),
    ]
}

/// What the signal's first lines are.
const SIGNAL_START: &str = "//! What a signal on the line shows, and what it tells a train to do.

/// What a signal on the line tells a train to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Clear,
    Caution,
    Stop,
}";

/// The lines `may_pass` adds to the signal, after the end of the function before it, with what's around them.
fn may_pass() -> Vec<HunkLine> {
    let same = |text: &str| HunkLine { mark: LineMark::Same, text: text.into() };
    let added = |text: &str| HunkLine { mark: LineMark::Added, text: text.into() };
    vec![
        same("        }"),
        same("    }"),
        added(""),
        added("    pub fn may_pass(self) -> bool {"),
        added("        self != Self::Stop"),
        added("    }"),
        same("}"),
    ]
}

/// Adding `may_pass` to the signal, and a depot that uses it.
fn patch() -> Patch {
    let update = PatchChange::Update { move_to: None, hunks: vec![PatchHunk { after: None, lines: may_pass() }] };
    Patch {
        files: vec![
            FilePatch { path: SIGNAL.into(), change: update },
            FilePatch { path: DEPOT.into(), change: PatchChange::Add { content: DEPOT_RS.into() } },
        ],
    }
}

/// What the patch did to the file at `path`: changed it where it `was` there, with the file's own line numbers, or
/// made it.
fn changed(path: &str, was: bool) -> FileOutcome {
    let change = if was {
        FileChange::Changed { hunks: vec![Hunk { old_start: 28, new_start: 28, lines: may_pass() }] }
    } else {
        FileChange::Created
    };
    FileOutcome { path: path.into(), change: Some(change), moved_to: None, diagnostics: Vec::new() }
}

/// Patches of one file each, which say what they did to it as an edit or a write does: a file added, one changed, one
/// moved, and one deleted.
pub(crate) fn one_file_patches() -> Vec<EventBody> {
    let reply = "The depot has its own module now, and the old yard is gone.";
    let one = |path: &str, change| Call::Patch(Patch { files: vec![FilePatch { path: path.into(), change }] });
    let moved = PatchChange::Update { move_to: Some(format!("{DEPOT_DIR}/sidings.rs")), hunks: Vec::new() };
    let update = PatchChange::Update { move_to: None, hunks: vec![PatchHunk { after: None, lines: may_pass() }] };
    let moved_back = FileOutcome {
        path: format!("{DEPOT_DIR}/yard/sidings.rs"),
        change: None,
        moved_to: Some(format!("{DEPOT_DIR}/sidings.rs")),
        diagnostics: Vec::new(),
    };
    vec![
        user_prompt(ASK),
        taken(Seq::new(1)),
        text("msg-1", "I'll give the depot a module of its own, and tidy the yard away."),
        typed_call("msg-2", "add-1", one(DEPOT, PatchChange::Add { content: DEPOT_RS.into() })),
        typed_result("add-1", CallOutput::Changed { files: vec![changed(DEPOT, false)] }, false),
        typed_call("msg-3", "edit-1", one(SIGNAL, update)),
        typed_result("edit-1", CallOutput::Changed { files: vec![changed(SIGNAL, true)] }, false),
        typed_call("msg-4", "move-1", one(&format!("{DEPOT_DIR}/yard/sidings.rs"), moved)),
        typed_result("move-1", CallOutput::Changed { files: vec![moved_back] }, false),
        typed_call("msg-5", "delete-1", one(&format!("{DEPOT_DIR}/yard.rs"), PatchChange::Delete)),
        typed_result("delete-1", CallOutput::Changed { files: Vec::new() }, false),
        text("msg-6", reply),
        replied(reply, 41, 0.09),
    ]
}

const DEPOT_DIR: &str = "/workspaces/aspen/crates/depot/src";

/// The line of a command that does nothing but search, drawn as searching.
pub(crate) const SEARCH_LINE: &str = "Search";

/// The line of a web search.
pub(crate) const WEB_LINE: &str = "Search the web";

/// Searches and calls of the web: a command that does nothing but search, drawn as searching, a search that only
/// counts, a find of files by name, a web search whose agent says only words, as Antigravity's does, and a fetch.
pub(crate) fn searched() -> Vec<EventBody> {
    let reply = "Only the train and the depot ask may_pass, and a flashing yellow should let a train pass.";
    let searches = vec![CommandPart::Search { pattern: Some("may_pass".into()), path: Some("crates".into()) }];
    let rg = Call::Command(Command { parts: searches, ..typed_command("rg -n may_pass crates") });
    let counted = Search { pattern: "Aspect::".into(), path: None, files: Some("*.rs".into()), shows: Shows::Count };
    let find = FindFiles { pattern: "**/*_test.rs".into(), path: None };
    let tests = ["crates/signals/src/signal_test.rs", "crates/depot/src/depot_test.rs"];
    let query = "flashing yellow railway signal";
    let web = WebSearch { query: query.into(), only_domains: Vec::new(), not_domains: Vec::new() };
    let url = "https://example.org/signals/aspects";
    vec![
        user_prompt("Where is may_pass asked, and should a flashing yellow pass?"),
        taken(Seq::new(1)),
        typed_call("msg-1", "call-1", rg),
        typed_result("call-1", command_output(FOUND, Some(0), Ended::Exited), false),
        typed_call("msg-2", "call-2", Call::Search(counted)),
        typed_result("call-2", CallOutput::Found(Found::Count { matches: 12, files: 4 }), false),
        typed_call("msg-3", "call-3", Call::FindFiles(find)),
        typed_result(
            "call-3",
            CallOutput::Paths(Paths { paths: tests.map(Into::into).into(), truncated: false }),
            false,
        ),
        typed_call("msg-4", "call-4", Call::WebSearch(web)),
        typed_result("call-4", CallOutput::Words { text: WEB_WORDS.into() }, false),
        typed_call("msg-5", "call-5", Call::Fetch(Fetch { url: url.into(), asking: None })),
        typed_result("call-5", CallOutput::Page { url: url.into(), status: Some(200), text: PAGE.into() }, false),
        text("msg-6", reply),
        replied(reply, 31, 0.12),
    ]
}

/// What the command that searched printed, as ripgrep prints its matches.
const FOUND: &str = "crates/signals/src/train.rs:53:        if signal.may_pass() {
crates/signals/src/train.rs:61:        debug_assert!(signal.may_pass());
crates/depot/src/lib.rs:4:    signal.may_pass()
";

/// What a web search said it found, in words alone.
const WEB_WORDS: &str = "A flashing yellow is a preliminary caution on most railways: the train may pass, and the \
                         next signal shows caution. Some railways use it to warn of a diverging route ahead.";

/// The page the fetch sent back.
const PAGE: &str = "Signal aspects\n\nStop, caution, preliminary caution and clear.";
