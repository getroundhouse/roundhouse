use crate::capability::calls::call::Read;
use crate::capability::calls::output::{FileOutcome, Found, Lines, McpContent};
use crate::capability::calls::{Call, CallOutput};
use crate::event::{ContentBlock, Denied, EventBody, ExitReason, FileChange, SessionEvent, TaskOutcome};
use crate::fixtures;
use crate::model::{Lately, Seq, Subagent, SubagentId, SubagentStatus, TouchedSubagents};

use super::lately::{CALLS_KEPT, RESULT_KEPT, TEXT_KEPT, kept};

/// By its ID first, then by its call, so one whose ID is another's call is the one found by it, and one named by neither
/// is none.
#[test]
fn a_subagent_is_named_by_its_id_or_else_by_its_call() {
    let named = |id: &str, call: &str| Subagent { id: Some(id.into()), ..fixtures::subagent(call) };
    let subagents = [named("agent-1", "toolu_1"), named("toolu_1-again", "agent-1"), fixtures::subagent("toolu_3")];
    let found = |name: &str| Subagent::named(&subagents, &name.into()).map(Subagent::id);

    assert_eq!(found("toolu_1"), Some(SubagentId::from("agent-1")), "by its call");
    assert_eq!(found("agent-1"), Some(SubagentId::from("agent-1")), "its ID, though it's another's call");
    assert_eq!(found("toolu_3"), Some(SubagentId::from("toolu_3")));
    assert_eq!(found("unknown"), None);
}

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

/// A read of the file named `id`.
fn call(id: &str) -> ContentBlock {
    fixtures::typed_call(id, Call::Read(Read { path: id.into(), lines: None, pages: None }))
}

fn result(id: &str, text: &str) -> ContentBlock {
    fixtures::typed_result(id, CallOutput::Words { text: text.into() }, false)
}

/// Each call's id, and the text kept just before it.
fn calls(work: &Lately) -> Vec<(&str, Option<&str>)> {
    work.calls.iter().map(|made| (made.id.as_str(), made.before.as_ref().map(|said| said.text.as_str()))).collect()
}

/// What came back for the call at `place`, which is words, and whether it was cut.
fn came(work: &Lately, place: usize) -> (&str, bool) {
    let result = work.calls[place].result.as_ref().unwrap();
    let CallOutput::Words { text } = &result.output else { panic!("{result:?}") };
    (text, result.cut)
}

#[test]
fn the_newest_calls_are_kept_each_with_the_text_just_before_it_then_what_it_has_said_since() {
    let mut work = Lately::default();

    assert!(work.add(&[text("First I'll look."), call("a")], true));
    assert!(work.add(&[call("b")], true));
    assert!(work.add(&[text("Then this."), text("And that."), call("c"), call("d")], true));
    assert!(work.add(&[text("Nearly done.")], true));

    assert_eq!(CALLS_KEPT, 3);
    assert_eq!(calls(&work), [("b", None), ("c", Some("Then this.\n\nAnd that.")), ("d", None)], "the oldest went");
    assert_eq!(work.since.as_ref().map(|said| said.text.as_str()), Some("Nearly done."));
    assert!(!work.add(&[text("  "), ContentBlock::Thinking { text: "Hm.".into() }], true), "nothing to show");
}

#[test]
fn a_call_s_result_is_kept_with_it_cut_short() {
    let mut work = Lately::default();
    work.add(&[call("short"), call("long")], true);
    let long = Vec::from_fn(RESULT_KEPT, |n| n.to_string()).join("\n");

    assert!(work.add(&[result("short", "one\ntwo"), result("long", &long)], true));
    assert!(!work.add(&[result("gone", "x")], true), "a call it doesn't keep");

    assert_eq!(came(&work, 0), ("one\ntwo", false));
    let (long, cut) = came(&work, 1);
    assert_eq!((long.chars().count(), cut), (RESULT_KEPT, true));
}

#[test]
fn a_long_text_keeps_its_start_whole_characters_and_all() {
    let mut work = Lately::default();
    let long = "한".repeat(TEXT_KEPT + 1);

    work.add(&[text(&long)], true);
    assert!(!work.add(&[text("More.")], true), "more of a text that's cut changes nothing, so sends no notice");

    let since = work.since.as_ref().unwrap();
    assert_eq!((since.text.chars().count(), since.cut), (TEXT_KEPT, true));
    assert!(since.text.chars().all(|c| c == '한'), "nothing more is added to a text that's cut");
}

/// Text in a `user` line was written to the subagent, such as the prompt the CLI sends one run in the foreground, so
/// only what came back is taken from one.
#[test]
fn only_what_came_back_is_taken_from_a_user_line() {
    let mut work = Lately::default();
    work.add(&[call("a")], true);

    assert!(!work.add(&[text("Find where the tests live.")], false), "its prompt");
    assert!(work.add(&[result("a", "one")], false));

    assert_eq!(work.since, None);
    assert_eq!(came(&work, 0), ("one", false));
}

/// So a client says a subagent's denied call was denied, with what the person said, from its record.
#[test]
fn a_denied_calls_result_says_so() {
    let mut work = Lately::default();

    work.add(&[fixtures::typed_call("c", Call::Command(fixtures::typed_command("rm -rf x")))], true);
    work.add(&[fixtures::typed_denied("c", "Not that", Some("Not that"))], true);

    assert_eq!(work.calls[0].result.as_ref().unwrap().denied, Some(Denied { said: Some("Not that".into()) }));
}

/// What came back for a typed call keeps each text it holds, in order, as far as [`RESULT_KEPT`] characters go between
/// them, saying it cut them, and never what it changed in a file. Everything else is as it came.
#[test]
fn a_typed_result_keeps_the_start_of_its_texts_between_them() {
    let read = CallOutput::Lines(Lines {
        first: 1,
        lines: vec!["x".repeat(RESULT_KEPT - 1), "ab".into(), "c".into()],
        total: Some(3),
    });
    let texts = |output: &CallOutput| -> Vec<String> {
        if let CallOutput::Lines(read) = output {
            return read.lines.clone();
        }
        let CallOutput::Mcp { content, .. } = output else { panic!("no texts in {output:?}") };
        content
            .iter()
            .filter_map(|block| if let McpContent::Text { text } = block { Some(text.clone()) } else { None })
            .collect()
    };
    let mcp = CallOutput::Mcp {
        content: vec![
            McpContent::Text { text: "y".repeat(RESULT_KEPT - 10) },
            McpContent::Text { text: "z".repeat(20) },
        ],
        structured: None,
    };
    let changed = |change| CallOutput::Changed {
        files: vec![FileOutcome { path: "a.rs".into(), change, moved_to: None, diagnostics: Vec::new() }],
    };
    let count = CallOutput::Found(Found::Count { matches: 3, files: 2 });

    let (lines, cut) = kept(&read);
    assert_eq!((texts(&lines).iter().map(String::len).collect::<Vec<_>>(), cut), (vec![RESULT_KEPT - 1, 1], true));
    let (mcp, cut) = kept(&mcp);
    assert_eq!((texts(&mcp).iter().map(String::len).collect::<Vec<_>>(), cut), (vec![RESULT_KEPT - 10, 10], true));
    assert_eq!(kept(&changed(Some(FileChange::Created))), (changed(None), false), "what it changed is left off");
    assert_eq!(kept(&count), (count, false));
}

/// The hub reads only the records an event touches, as `Subagent::touched_by` says, notes the event over those, and
/// writes back those it changed. So that has to change what noting it over every record would, and say the same
/// changed.
#[test]
fn an_event_changes_only_the_records_it_touches() {
    let start = |call: &str, id: &str| {
        let mut start = fixtures::subagent_started(call, false);
        if let EventBody::SubagentStarted { subagent, .. } = &mut start {
            *subagent = Some(id.into());
        }
        start
    };
    let message = |blocks| EventBody::Assistant {
        message_id: None,
        subagent: None,
        model: Some("claude-haiku-5-5".into()),
        blocks,
        first_index: None,
    };
    let ended = |id: &str| EventBody::SubagentEnded {
        subagent: id.into(),
        outcome: TaskOutcome::Finished,
        report: Some("Done.".into()),
        context_tokens: None,
    };
    let exited =
        EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] };
    let log = [
        (None, start("toolu_1", "thread-1")),
        (None, start("toolu_2", "thread-2")),
        (Some("thread-2"), message(vec![text("I'll read it."), call("read")])),
        (Some("thread-2"), EventBody::AgentUser { subagent: None, blocks: vec![result("read", "fn main() {}")] }),
        (None, EventBody::SubagentContext { subagent: "thread-1".into(), tokens: 1_200 }),
        (None, EventBody::SubagentContext { subagent: "thread-1".into(), tokens: 1_200 }),
        (None, start("toolu_3", "thread-1")),
        (None, start("toolu_1", "thread-3")),
        (None, ended("thread-2")),
        (Some("thread-2"), message(vec![call("late")])),
        (None, start("toolu_send", "thread-2")),
        (None, exited),
        (None, start("toolu_4", "thread-4")),
    ];

    let (mut every, mut kept) = (Vec::new(), Vec::new());
    for (seq, (from, body)) in log.into_iter().enumerate() {
        let event = SessionEvent {
            subagent: from.map(Into::into),
            ts_ms: i64::try_from(seq).unwrap(),
            ..fixtures::event(Seq::new(u64::try_from(seq).unwrap()), body)
        };
        let noted: Vec<SubagentId> = Subagent::note(&mut every, &event).iter().map(|it| it.id()).collect();
        assert_eq!(noted_as_the_hub_does(&mut kept, &event), noted, "{event:?}");
        assert_eq!(kept, every, "{event:?}");
    }
    assert_eq!(kept.len(), 3, "a call starts only one");
}

/// Notes `event` in `kept` as the hub does, reading only the records it touches and writing back those it changed, and
/// says which changed.
fn noted_as_the_hub_does(kept: &mut Vec<Subagent>, event: &SessionEvent) -> Vec<SubagentId> {
    let touches = |it: &&Subagent| match Subagent::touched_by(event) {
        Some(TouchedSubagents::One(id)) => it.id() == id,
        Some(TouchedSubagents::Starting { subagent, call }) => it.id() == subagent || it.call == call,
        Some(TouchedSubagents::Working) => it.status == SubagentStatus::Working,
        None => false,
    };
    let mut read: Vec<Subagent> = kept.iter().filter(touches).cloned().collect();
    let changed: Vec<Subagent> = Subagent::note(&mut read, event).into_iter().map(|it| it.clone()).collect();
    let ids = changed.iter().map(Subagent::id).collect();
    for it in changed {
        match kept.iter_mut().find(|held| held.id() == it.id()) {
            Some(held) => *held = it,
            None => kept.push(it),
        }
    }
    ids
}
