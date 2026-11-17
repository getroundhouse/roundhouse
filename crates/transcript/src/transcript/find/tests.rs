//! How what's counted follows what the log holds, and what the bar says of it.

use std::collections::BTreeSet;
use std::sync::Arc;

use client_core::session_log::{Position, SessionLog};
use protocol::capability::calls::call::{
    CellEdit, Command, CommandPart, Fetch, FilePatch, FindFiles, NotebookEdit, Patch, PatchChange, PatchHunk, Read,
    Search, Shows, WebSearch,
};
use protocol::capability::calls::output::{Ended, FileOutcome, Found, FoundLine, Lines, Paths, WebResult};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody, FileChange, Hunk, HunkLine, LineMark, SessionEvent};
use protocol::find::Needle;
use protocol::model::{Place, Rev, Seq, Subagent, SubagentStatus};
use protocol::prompt::Prompt;

use super::count::{At, Counts};
use super::stretch::{Known, counted_in};
use super::{More, changed_to_count, count_held, first_match, held_to_count, says};
use crate::fixtures::{event, log, record, subagents, within};
use crate::transcript::{Model, SubagentRecords};

fn needle(query: &str) -> Needle {
    Needle::new(query).unwrap()
}

fn agent_says(text: &str) -> EventBody {
    let blocks = vec![ContentBlock::Text { text: text.into(), images: Vec::new() }];
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

fn runs(id: &str, command: &str) -> EventBody {
    let call = Call::Command(protocol::fixtures::typed_command(command));
    let blocks = vec![protocol::fixtures::typed_call(id, call)];
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

fn printed(id: &str, text: &str) -> EventBody {
    let output = protocol::fixtures::command_output(text, Some(0), Ended::Exited);
    EventBody::AgentUser { subagent: None, blocks: vec![protocol::fixtures::typed_result(id, output, false)] }
}

/// `body` at `place` among the agent's own events, with a `seq` far from it, as the agent's are once a subagent has
/// worked.
fn placed(place: u64, body: EventBody) -> SessionEvent {
    SessionEvent { place: Place::new(place), ..event(Seq::new(place * 3), body) }
}

/// The agent's own events, by place, as a chat follows them.
fn own(events: impl IntoIterator<Item = SessionEvent>) -> SessionLog {
    let mut log = SessionLog::by_place(Place::default());
    for event in events {
        log.apply_event(event);
    }
    log
}

/// The record of the subagent the call `call` started, done, with `report`.
fn reported(call: &str, report: &str) -> SubagentRecords {
    let record = Subagent {
        ended_at_ms: Some(1),
        calls: 1,
        report: Some(report.into()),
        rev: Rev::default(),
        ..record(call, SubagentStatus::Finished)
    };
    let mut records = SubagentRecords::default();
    records.insert(Arc::new(record));
    records
}

/// Whatever else happened, so the events that matter are on pages of their own.
fn filler(seqs: std::ops::Range<u64>) -> impl Iterator<Item = SessionEvent> {
    seqs.map(|seq| event(Seq::new(seq), agent_says("nothing to see")))
}

/// Counts `rows` of `log` into `counts`, as the transcript does.
fn count(log: &SessionLog, m: &Model, needle: &Needle, rows: BTreeSet<usize>, counts: &mut Counts) {
    for (row, count) in count_held(log, m, needle, rows) {
        counts.set(row, count);
    }
}

/// Counts again what changes when `changed` rows of `log` do, as the transcript does.
fn follow(
    log: &SessionLog,
    m: &Model,
    needle: &Needle,
    candidates: &mut BTreeSet<Position>,
    counts: &mut Counts,
    changed: std::ops::Range<usize>,
) {
    let (found, rows) = changed_to_count(log, m, (needle, candidates, counts), changed);
    candidates.extend(found);
    count(log, m, needle, rows, counts);
}

#[test]
fn what_the_log_holds_is_counted_by_the_rows_that_draw_it() {
    let log = own([
        placed(1, agent_says("the lexer")),
        placed(2, agent_says("the parser")),
        placed(3, runs("run", "cargo test")),
    ]);
    let m = Model::default();

    let (found, rows) = held_to_count(&log, &m, &needle("parser"));

    assert_eq!(found, [Position::of_row(1)], "in its place's row, not its seq's");
    assert_eq!(rows.into_iter().collect::<Vec<_>>(), [1], "only the rows of what holds it");
}

#[test]
fn a_result_that_comes_after_its_call_is_counted_in_the_call_s_row() {
    let needle = needle("failed");
    let m = Model::default();
    let (mut candidates, mut counts) = (BTreeSet::new(), Counts::default());
    let before = log([event(Seq::new(1), runs("run", "cargo test"))]);
    follow(&before, &m, &needle, &mut candidates, &mut counts, 0..1);
    assert_eq!(counts.total(), 0);

    let after = log([
        event(Seq::new(1), runs("run", "cargo test")),
        event(Seq::new(2), printed("run", "test parser ... FAILED")),
    ]);
    follow(&after, &m, &needle, &mut candidates, &mut counts, 1..2);

    assert_eq!(counts.of(0), 1, "the call draws what came back");
    assert_eq!(counts.total(), 1);
}

#[test]
fn a_subagent_s_work_held_before_its_call_moves_into_the_call_once_it_loads() {
    let needle = needle("parser");
    let m = Model::default();
    let work = || event(Seq::new(300), within("helper", agent_says("Looking at the parser")));
    let (mut candidates, mut counts) = (BTreeSet::new(), Counts::default());
    // Its page loads first, without the call, so it's drawn in its own row.
    follow(&log([work()]), &m, &needle, &mut candidates, &mut counts, 299..300);
    assert_eq!(counts.of(299), 1);

    // Then the call's page, which draws it, far from where its page changed.
    let both = log(std::iter::once(event(Seq::new(1), subagents(&["helper"]))).chain(filler(2..300)).chain([work()]));
    follow(&both, &m, &needle, &mut candidates, &mut counts, 0..250);

    assert_eq!((counts.of(0), counts.of(299)), (1, 0), "in the call now, and not where it was");
    assert_eq!(counts.total(), 1, "once");
}

#[test]
fn a_subagent_s_call_counts_the_report_its_record_has_as_it_draws_it() {
    let needle = needle("parser");
    let call = || placed(1, subagents(&["helper"]));

    let known = Known { records: reported("helper", "The parser is fixed"), ..Known::default() };
    let read = counted_in(vec![call()], &[Position::of_row(0)], &needle, known);
    let subagents = reported("helper", "The parser is fixed");
    let m = Model { subagents, ..Model::default() };
    let mut counts = Counts::default();
    count(&own([call()]), &m, &needle, [0].into(), &mut counts);

    assert_eq!(read, [(0, 1)], "read in a stretch");
    assert_eq!(counts.of(0), 1, "and held");
    let unreported = counted_in(vec![call()], &[Position::of_row(0)], &needle, Known::default());
    assert!(unreported.iter().all(|(_, n)| *n == 0), "with no report");
}

#[test]
fn a_row_the_log_lets_go_of_keeps_what_it_was_counted_as() {
    let needle = needle("parser");
    let mut counts = Counts::default();
    counts.set(4, 2);
    let held = log([event(Seq::new(1), agent_says("the parser"))]);

    follow(&held, &Model::default(), &needle, &mut BTreeSet::new(), &mut counts, 0..10);

    assert_eq!((counts.of(0), counts.of(4)), (1, 2));
}

#[test]
fn a_stretch_counts_what_came_back_in_its_call_s_row_by_place() {
    let events = vec![placed(1, runs("run", "cargo test")), placed(2, printed("run", "test parser ... FAILED"))];

    let counted = counted_in(events, &[Position::of_row(1)], &needle("failed"), Known::default());

    assert_eq!(counted, [(0, 1), (1, 0)]);
}

/// A stretch puts a prompt where the chat does, which is where it was settled, rather than counting it in its own row,
/// which draws nothing of it. Settled past the end of the stretch, it's in no row the stretch holds.
#[test]
fn a_stretch_counts_a_prompt_where_the_chat_draws_it() {
    let prompt = |place| {
        let body = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("the parser") };
        placed(place, body)
    };
    // A prompt's `seq` is three times its place.
    let taken = |place, prompt_place: u64| placed(place, EventBody::PromptTaken { prompt: Seq::new(prompt_place * 3) });
    let says = |place, text| placed(place, agent_says(text));
    let chat = own([prompt(1), taken(2, 1), says(3, "first"), prompt(4), says(5, "second"), taken(6, 4)]);
    let known = || Known { settled: chat.settled_prompts(), ..Known::default() };

    let fourth = [Position::of_row(3)];
    let within = counted_in(vec![prompt(4), says(5, "second"), taken(6, 4)], &fourth, &needle("parser"), known());
    let past_its_end = counted_in(vec![prompt(4), says(5, "second")], &fourth, &needle("parser"), known());

    assert_eq!(within, [(3, 0), (5, 1)], "in the row that took it, not its own");
    assert_eq!(past_its_end, [(3, 0)], "nowhere it holds");
}

#[test]
fn the_first_match_is_shown_once_nothing_above_it_can_hold_another() {
    let mut counts = Counts::default();
    counts.set(10, 1);
    counts.set(50, 1);
    let around_it = log((6..=11).map(|seq| event(Seq::new(seq), agent_says("nothing to see"))));
    let only_it = log([event(Seq::new(11), agent_says("nothing to see"))]);

    assert_eq!(first_match(&counts, 5, false, &around_it), Some(At { row: 10, nth: 0 }), "every row above it is held");
    assert_eq!(first_match(&counts, 5, false, &only_it), None, "one above it may hold another");
    assert_eq!(first_match(&counts, 5, true, &only_it), Some(At { row: 10, nth: 0 }), "until every match is counted");
    assert_eq!(first_match(&counts, 60, false, &around_it), None);
    assert_eq!(
        first_match(&counts, 60, true, &around_it),
        Some(At { row: 10, nth: 0 }),
        "none below: the first of all"
    );
}

#[test]
fn the_bar_says_whether_there_may_be_more() {
    assert_eq!(says(Some(0), 3, More::No), "1 of 3");
    assert_eq!(says(Some(0), 3, More::Counting), "1 of 3…");
    assert_eq!(says(Some(0), 3, More::Unlooked), "1 of 3+");
    assert_eq!(says(None, 2, More::Unlooked), "2 found+");
    assert_eq!(says(None, 0, More::No), "No matches");
    assert_eq!(says(None, 0, More::Counting), "Finding…");
    assert_eq!(says(None, 0, More::Unlooked), "No matches loaded");
}

/// The agent making the typed call `call`, as `id`.
fn typed(id: &str, call: Call) -> EventBody {
    let blocks = vec![protocol::fixtures::typed_call(id, call)];
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

/// What came back for the typed call `id`.
fn came_back(id: &str, output: CallOutput) -> EventBody {
    EventBody::AgentUser { subagent: None, blocks: vec![protocol::fixtures::typed_result(id, output, false)] }
}

/// What came back for the typed call `id`, which failed.
fn failed_back(id: &str, output: CallOutput) -> EventBody {
    EventBody::AgentUser { subagent: None, blocks: vec![protocol::fixtures::typed_result(id, output, true)] }
}

fn added(text: &str) -> Vec<HunkLine> {
    vec![HunkLine { mark: LineMark::Added, text: text.into() }]
}

/// A typed call of a file is found where its row draws it: a read's lines, each file a patch changed, as what came
/// back says it changed it once it does, or deleted or moved, what a command that only reads printed, unless it went
/// on in the background, and what a notebook's cell holds afterwards.
#[test]
fn a_typed_file_call_is_found_as_its_row_draws_it() {
    let read = Call::Read(Read { path: "src/signal.rs".into(), lines: None, pages: None });
    let lines = CallOutput::Lines(Lines { first: 40, lines: vec!["fn may_pass(&self) -> bool {".into()], total: None });
    let update =
        PatchChange::Update { move_to: None, hunks: vec![PatchHunk { after: None, lines: added("worked out") }] };
    let patch = Call::Patch(Patch {
        files: vec![
            FilePatch { path: "src/signal.rs".into(), change: update },
            FilePatch { path: "src/depot.rs".into(), change: PatchChange::Add { content: "may_pass".into() } },
            FilePatch { path: "src/yard.rs".into(), change: PatchChange::Delete },
            FilePatch {
                path: "src/sidings.rs".into(),
                change: PatchChange::Update { move_to: Some("src/yard/sidings.rs".into()), hunks: Vec::new() },
            },
        ],
    });
    let hunks = vec![Hunk { old_start: 3, new_start: 3, lines: added("may_pass, as it came back") }];
    let signal = FileOutcome {
        path: "src/signal.rs".into(),
        change: Some(FileChange::Changed { hunks }),
        moved_to: None,
        diagnostics: Vec::new(),
    };
    let reads = vec![CommandPart::Read { path: "notes.md".into() }];
    let cat = Call::Command(Command { parts: reads, ..protocol::fixtures::typed_command("cat notes.md") });
    let printed = protocol::fixtures::command_output("may_pass is unchanged", Some(0), Ended::Exited);
    let tailed = vec![CommandPart::Read { path: "depot.log".into() }];
    let tail = Call::Command(Command { parts: tailed, ..protocol::fixtures::typed_command("tail -f depot.log") });
    let went_on = Ended::Backgrounded { task: "tail".into() };
    let tailing = protocol::fixtures::command_output("depot up", None, went_on);
    let cell =
        NotebookEdit { path: "a.ipynb".into(), cell: None, source: "may_pass()".into(), does: CellEdit::Replace };
    let log = own([
        placed(1, typed("read", read)),
        placed(2, came_back("read", lines)),
        placed(3, typed("patch", patch)),
        placed(4, came_back("patch", CallOutput::Changed { files: vec![signal] })),
        placed(5, typed("cat", cat)),
        placed(6, came_back("cat", printed)),
        placed(7, typed("cell", Call::NotebookEdit(cell))),
        placed(8, typed("tail", tail)),
        placed(9, came_back("tail", tailing)),
    ]);
    let counted = |query| {
        let counts = count_held(&log, &Model::default(), &needle(query), (0..9).collect());
        counts.into_iter().filter(|(_, count)| *count > 0).collect::<Vec<_>>()
    };

    assert_eq!(counted("may_pass"), [(0, 1), (2, 2), (4, 1), (6, 1)]);
    assert_eq!(counted("worked out"), [], "what came back says what the patch did");
    assert_eq!(counted("yard"), [(2, 2)], "a file it deleted, and where one it moved went");
    assert_eq!(counted("cat "), [], "a command that only reads is drawn as reading, without the command");
    assert_eq!(counted("replace"), [], "a notebook's cell is drawn as what it holds, not as the call's input");
    assert_eq!(counted("tail -f"), [(7, 1)], "one that went on in the background is drawn as a command, which says so");
}

/// A typed search, a command that only searches, and a call of the web are found where their rows draw them: a
/// search's files and lines, what a command that only searches printed, without the command, a page's address, what
/// was asked of it and what came back, and a web search's pages and what was made of them, or its words alone.
#[test]
fn a_typed_search_or_call_of_the_web_is_found_as_its_row_draws_it() {
    let search = Call::Search(Search { pattern: "may_pass".into(), path: None, files: None, shows: Shows::Lines });
    let line = FoundLine { path: "src/signal.rs".into(), line: 3, text: "fn may_pass() {".into() };
    let lines = CallOutput::Found(Found::Lines { lines: vec![line], truncated: false });
    let find = Call::FindFiles(FindFiles { pattern: "**/may_pass*".into(), path: None });
    let paths = CallOutput::Paths(Paths { paths: vec!["tests/may_pass.rs".into()], truncated: false });
    let searches = vec![CommandPart::Search { pattern: Some("may_pass".into()), path: None }];
    let rg = Call::Command(Command { parts: searches, ..protocol::fixtures::typed_command("rg -n may_pass") });
    let printed = protocol::fixtures::command_output("src/depot.rs:8:may_pass();\n", Some(0), Ended::Exited);
    let url = "https://example.org/may_pass";
    let fetch = Call::Fetch(Fetch { url: url.into(), asking: Some("Does may_pass block?".into()) });
    let page = CallOutput::Page { url: url.into(), status: Some(200), text: "may_pass never blocks.".into() };
    let web =
        |query: &str| Call::WebSearch(WebSearch { query: query.into(), only_domains: vec![], not_domains: vec![] });
    let found = WebResult { title: "may_pass, explained".into(), url: "https://example.org/a".into(), snippet: None };
    let pages = CallOutput::WebResults { results: vec![found], summary: Some("It checks may_pass.".into()) };
    let words = CallOutput::Words { text: "may_pass is the signal's check.".into() };
    let gone = vec![CommandPart::Search { pattern: Some("may_pass".into()), path: Some("gone".into()) }];
    let failed = Call::Command(Command { parts: gone, ..protocol::fixtures::typed_command("rg -n may_pass gone") });
    let missing = protocol::fixtures::command_output("rg: gone: No such file or directory", Some(2), Ended::Exited);
    let log = own([
        placed(1, typed("search", search)),
        placed(2, came_back("search", lines)),
        placed(3, typed("find", find)),
        placed(4, came_back("find", paths)),
        placed(5, typed("rg", rg)),
        placed(6, came_back("rg", printed)),
        placed(7, typed("fetch", fetch)),
        placed(8, came_back("fetch", page)),
        placed(9, typed("web", web("signals"))),
        placed(10, came_back("web", pages)),
        placed(11, typed("said", web("aspects"))),
        placed(12, came_back("said", words)),
        placed(13, typed("failed", failed)),
        placed(14, failed_back("failed", missing)),
    ]);
    let counted = |query| {
        let counts = count_held(&log, &Model::default(), &needle(query), (0..14).collect());
        counts.into_iter().filter(|(_, count)| *count > 0).collect::<Vec<_>>()
    };

    assert_eq!(counted("may_pass"), [(0, 2), (2, 2), (4, 2), (6, 3), (8, 2), (10, 1), (12, 1)]);
    assert_eq!(counted("No such file"), [(12, 1)], "what went wrong, under a search that failed");
    assert_eq!(counted("signal.rs"), [(0, 1)], "the file a match is in");
    assert_eq!(counted("rg -n"), [], "a command that only searches is drawn as searching, without the command");
}
