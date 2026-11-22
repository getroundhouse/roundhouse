use super::super::*;
use crate::store::lock;
use crate::store::now_ms;
use crate::store::tests::store_with_session;
use protocol::capability::calls::call::Read;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ExitReason, TaskOutcome};
use protocol::fixtures;
use protocol::model::SubagentStatus::{Finished, Stopped, Working};

pub(super) fn started(call: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        subagent: None,
        kind: Some("Explore".into()),
        description: Some("Find the tests".into()),
        prompt: Some("Find where the tests live.".into()),
        in_background: false,
    }
}

pub(super) fn ended(call: &str, outcome: TaskOutcome, report: Option<&str>) -> EventBody {
    EventBody::SubagentEnded { subagent: call.into(), outcome, report: report.map(Into::into), context_tokens: None }
}

/// Reading the file named `id`.
pub(super) fn read(id: &str) -> Call {
    Call::Read(Read { path: id.into(), lines: None, pages: None })
}

/// The call [`read`] makes, as the block it comes in.
pub(super) fn reading(id: &str) -> ContentBlock {
    fixtures::typed_call(id, read(id))
}

/// A message from the agent, or from the subagent `from` started.
pub(super) fn message(from: Option<&str>, blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, subagent: from.map(Into::into), model: None, blocks, first_index: None }
}

/// A reply from `model`, from the agent, or from the subagent `from` started.
pub(super) fn reply(from: Option<&str>, model: &str, blocks: Vec<ContentBlock>) -> EventBody {
    let mut reply = message(from, blocks);
    if let EventBody::Assistant { model: named, .. } = &mut reply {
        *named = Some(model.into());
    }
    reply
}

pub(super) fn model(store: &Store, call: &str) -> Option<String> {
    subagent(store, call).model.map(String::from)
}

pub(super) fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] }
}

/// Records `body` in the session `s`, and says which subagents' records it changed.
pub(super) fn record(store: &Store, body: &EventBody) -> (SessionEvent, Vec<String>) {
    let appended = store.append(&"s".into(), None, body, None, None, None).unwrap();
    (appended.event, appended.subagents.iter().map(|call| call.as_str().to_owned()).collect())
}

pub(super) fn subagent(store: &Store, call: &str) -> Subagent {
    store.get_subagent(&"s".into(), &call.into()).unwrap().unwrap()
}

#[test]
fn a_subagents_record_starts_with_what_it_was_asked_counts_its_calls_and_ends_with_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let (start, changed) = record(&store, &started("agent"));
    assert_eq!(changed, ["agent"]);
    let first = subagent(&store, "agent");
    assert_eq!((first.status, first.started_at_ms, first.calls, &first.lately), (Working, start.ts_ms, 0, &None));
    assert_eq!((first.kind.as_deref(), first.prompt.as_deref()), (Some("Explore"), Some("Find where the tests live.")));

    assert_eq!(record(&store, &message(None, vec![reading("own")])).1, Vec::<String>::new(), "the agent's own");
    assert_eq!(record(&store, &message(Some("other"), vec![reading("x")])).1, Vec::<String>::new(), "no record");
    let text = vec![ContentBlock::Thinking { text: "x".into() }];
    assert_eq!(record(&store, &message(Some("agent"), text)).1, Vec::<String>::new(), "nothing it shows");
    let blocks =
        vec![reading("read-1"), ContentBlock::Text { text: "x".into(), images: Vec::new() }, reading("read-2")];
    assert_eq!(record(&store, &message(Some("agent"), blocks)).1, ["agent"]);
    let working = subagent(&store, "agent");
    assert_eq!(working.calls, 2);
    assert_eq!(lately(&store, "agent"), (None, vec![("read-1".into(), None), ("read-2".into(), None)]));
    assert_eq!(working.lately.as_ref().unwrap().calls[1].call, read("read-2"), "as it was made");
    assert!(working.rev > first.rev, "a client only takes a newer copy");
    assert_eq!(record(&store, &started("agent")).1, Vec::<String>::new(), "it's working already");
    assert_eq!(subagent(&store, "agent").rev, working.rev);

    let (end, changed) = record(&store, &ended("agent", TaskOutcome::Finished, Some("They're in tests/.")));
    assert_eq!(changed, ["agent"]);
    let finished = subagent(&store, "agent");
    assert_eq!((finished.status, finished.ended_at_ms), (Finished, Some(end.ts_ms)));
    assert_eq!((finished.calls, finished.report.as_deref()), (2, Some("They're in tests/.")));
    assert!(finished.rev > working.rev);

    assert_eq!(record(&store, &ended("agent", TaskOutcome::Failed, None)).1, Vec::<String>::new(), "ended once");
    assert_eq!(subagent(&store, "agent"), finished);
}

/// A subagent the agent says started again, as Claude's does when it's messaged after it ended, works again, under the
/// call that first started it, which its row is drawn for, rather than the call that messaged it.
#[test]
fn a_subagent_that_starts_again_after_it_ended_works_again_under_the_call_that_first_started_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let start = |call: &str, in_background| {
        let EventBody::SubagentStarted { kind, description, prompt, .. } = started(call) else { unreachable!() };
        let subagent = Some("task-1".into());
        EventBody::SubagentStarted { call: call.into(), subagent, kind, description, prompt, in_background }
    };
    record(&store, &start("toolu_agent", false));
    record(&store, &message(Some("task-1"), vec![reading("read")]));
    record(&store, &ended("task-1", TaskOutcome::Finished, Some("They're in tests/.")));
    let finished = subagent(&store, "task-1");
    let (between, _) = record(&store, &started("between"));
    // So it starts again later than it ended, as it would after any time finished, and than another started since.
    let later = || now_ms() > between.ts_ms;
    waiting::thread::until(std::time::Duration::from_secs(10), "the clock stood still", later);

    let (restart, changed) = record(&store, &start("toolu_send", true));
    assert_eq!(changed, ["task-1"]);

    let again = subagent(&store, "task-1");
    assert_eq!((again.status, again.ended_at_ms, again.report), (Working, None, None), "its next report is its own");
    assert_eq!((again.call.as_str(), again.calls, again.in_background), ("toolu_agent", 1, true));
    assert_eq!(again.prompt, finished.prompt, "as it first started");
    assert_eq!(again.started_at_ms, restart.ts_ms, "at it from when it started again");
    assert!(again.rev > finished.rev);
    let (subagents, _) = store.list_subagents(&"s".into()).unwrap();
    let listed: Vec<SubagentId> = subagents.iter().map(Subagent::id).collect();
    assert_eq!(
        listed,
        [SubagentId::from("between"), "task-1".into()],
        "by when each last started, and none for the call that messaged it"
    );
    let mut other = started("toolu_agent");
    if let EventBody::SubagentStarted { subagent, .. } = &mut other {
        *subagent = Some("task-2".into());
    }
    assert_eq!(record(&store, &other).1, Vec::<String>::new(), "a call starts only one");
}

/// A subagent its agent gives an ID of its own is kept by that ID, which its events and its end name, with the call that
/// started it beside it. It's found by either, since a client from before subagents had IDs names each by its call, as
/// `Subagent::named` says, which a fake hub finds one by, so the store finds each as it does, and one whose ID is
/// another's call by its ID.
#[test]
fn a_subagent_named_by_its_agent_is_kept_by_that_name_and_found_by_its_call_too() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let start = |call: &str, id: &str| {
        let mut start = started(call);
        if let EventBody::SubagentStarted { subagent, .. } = &mut start {
            *subagent = Some(id.into());
        }
        start
    };

    assert_eq!(record(&store, &start("toolu_9", "thread-1")).1, ["thread-1"]);
    assert_eq!(record(&store, &message(Some("thread-1"), vec![reading("read")])).1, ["thread-1"]);
    assert_eq!(record(&store, &message(Some("toolu_9"), vec![reading("x")])).1, Vec::<String>::new(), "by its call");
    record(&store, &ended("thread-1", TaskOutcome::Finished, Some("Done.")));

    let kept = subagent(&store, "thread-1");
    assert_eq!((kept.id(), kept.call.as_str()), ("thread-1".into(), "toolu_9"));
    assert_eq!((kept.calls, kept.status, kept.report.as_deref()), (1, Finished, Some("Done.")));
    let named = |name: &str| store.subagent_named(&"s".into(), &name.into()).unwrap();
    assert_eq!((named("thread-1"), named("toolu_9")), ("thread-1".into(), "thread-1".into()));
    assert_eq!(named("unknown"), "unknown".into(), "one it doesn't know is taken as it is");
    record(&store, &start("toolu_10", "toolu_9"));
    let (listed, _) = store.list_subagents(&"s".into()).unwrap();
    for name in ["thread-1", "toolu_9", "toolu_10", "unknown"] {
        let recorded = store.recorded_subagent(&"s".into(), &name.into()).unwrap();
        assert_eq!(recorded, Subagent::named(&listed, &name.into()).map(Subagent::id), "named {name}");
    }
    assert_eq!(named("toolu_9"), "toolu_9".into(), "by its ID, though it's another's call");
}

#[test]
fn a_subagents_record_names_the_model_of_its_newest_reply_and_changes_only_when_that_does() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("agent"));
    let thinking = || vec![ContentBlock::Thinking { text: "x".into() }];
    let unchanged = Vec::<String>::new();

    assert_eq!(model(&store, "agent"), None, "before it's replied");
    let (_, changed) = record(&store, &reply(Some("agent"), "claude-haiku-5-5", thinking()));
    assert_eq!(changed, ["agent"], "even a reply that's only thinking says its model");
    let on_haiku = subagent(&store, "agent");
    assert_eq!(model(&store, "agent").as_deref(), Some("claude-haiku-5-5"));

    assert_eq!(record(&store, &message(Some("agent"), thinking())).1, unchanged, "a reply that names none");
    assert_eq!(record(&store, &reply(Some("agent"), "not a model", thinking())).1, unchanged, "a name that names none");
    assert_eq!(record(&store, &reply(None, "claude-opus-5-5", thinking())).1, unchanged, "the agent's own");
    assert_eq!(subagent(&store, "agent"), on_haiku, "it keeps its revision, so it isn't sent again");

    let (_, changed) = record(&store, &reply(Some("agent"), "claude-sonnet-5-5", vec![reading("read")]));
    assert_eq!(changed, ["agent"], "once");
    let on_sonnet = subagent(&store, "agent");
    assert_eq!((on_sonnet.model.map(String::from).as_deref(), on_sonnet.calls), (Some("claude-sonnet-5-5"), 1));
    assert!(on_sonnet.rev > on_haiku.rev);
}

#[test]
fn a_subagents_record_counts_what_its_conversation_holds_and_changes_only_when_the_count_does() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("agent"));
    let holds = |call: &str, tokens| EventBody::SubagentContext { subagent: call.into(), tokens };
    let unchanged = Vec::<String>::new();

    assert_eq!(subagent(&store, "agent").context_tokens, None, "before the agent says");
    assert_eq!(record(&store, &holds("agent", 1_200)).1, ["agent"]);
    let counted = subagent(&store, "agent");
    assert_eq!(counted.context_tokens, Some(1_200));

    assert_eq!(record(&store, &holds("agent", 1_200)).1, unchanged, "the count it had");
    assert_eq!(record(&store, &holds("nobody", 7)).1, unchanged, "one the agent never said it started");
    assert_eq!(subagent(&store, "agent"), counted, "it keeps its revision, so it isn't sent again");
    assert_eq!(record(&store, &holds("agent", 900)).1, ["agent"]);
    assert_eq!(subagent(&store, "agent").context_tokens, Some(900), "the newest count, which can be less");
    record(&store, &holds("agent", u64::MAX));
    assert_eq!(subagent(&store, "agent").context_tokens, Some(u64::MAX), "however large the agent says");

    record(&store, &holds("agent", 900));
    assert_eq!(record(&store, &ended("agent", TaskOutcome::Stopped, None)).1, ["agent"]);
    assert_eq!(subagent(&store, "agent").context_tokens, Some(900), "an end that counts none leaves its count");
    assert_eq!(record(&store, &holds("agent", 5_000)).1, unchanged, "it's ended, and its end has the last word");

    record(&store, &started("other"));
    record(&store, &holds("other", 10));
    let end = EventBody::SubagentEnded {
        subagent: "other".into(),
        outcome: TaskOutcome::Finished,
        report: None,
        context_tokens: Some(2_000),
    };
    record(&store, &end);
    assert_eq!(subagent(&store, "other").context_tokens, Some(2_000), "the count as it ended");
}

/// A record that changes keeps what a newer build stored in it that this one can't read, such as a status it doesn't
/// know or what it's done lately in another form, unless the rule sets that itself.
#[test]
fn a_record_keeps_what_a_newer_build_stored_in_it_when_another_field_of_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("newer"));
    record(&store, &started("working"));
    let stored = |id: &str| -> (String, Option<String>) {
        let sql = "SELECT status, lately FROM subagents WHERE id = ?";
        lock(&store.writer).query_row(sql, [id], |row| Ok((row.get(0)?, row.get(1)?))).unwrap()
    };
    let newer_form = r#""a newer form""#;
    lock(&store.writer)
        .execute_batch(&format!(
            "UPDATE subagents SET lately = '{newer_form}';
             UPDATE subagents SET status = 'pondering' WHERE id = 'newer';"
        ))
        .unwrap();

    let reply = reply(Some("newer"), "claude-haiku-5-5", vec![reading("read")]);
    assert_eq!(record(&store, &reply).1, ["newer"]);
    let holds = EventBody::SubagentContext { subagent: "working".into(), tokens: 1_200 };
    assert_eq!(record(&store, &holds).1, ["working"]);

    let newer = subagent(&store, "newer");
    assert_eq!((newer.calls, model(&store, "newer").as_deref()), (1, Some("claude-haiku-5-5")));
    assert_eq!(stored("newer"), ("pondering".into(), Some(newer_form.into())), "its status and what it did");
    assert_eq!(subagent(&store, "working").context_tokens, Some(1_200));
    assert_eq!(stored("working"), ("working".into(), Some(newer_form.into())), "what it did");
}

fn said(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

/// What came back for the call `id`, as words, to the agent, or to the subagent `to` started.
pub(super) fn answer(to: Option<&str>, id: &str, text: &str) -> EventBody {
    let result = fixtures::typed_result(id, CallOutput::Words { text: text.into() }, false);
    EventBody::AgentUser { subagent: to.map(Into::into), blocks: vec![result] }
}

/// What the subagent has said since its newest call, and each of its calls kept, with the words that came back for
/// it.
pub(super) fn lately(store: &Store, call: &str) -> (Option<String>, Vec<(String, Option<String>)>) {
    let work = subagent(store, call).lately.unwrap_or_default();
    let words = |output: CallOutput| {
        let CallOutput::Words { text } = &output else { panic!("{output:?}") };
        text.clone()
    };
    let calls =
        work.calls.into_iter().map(|made| (made.id.to_string(), made.result.map(|r| words(r.output)))).collect();
    (work.since.map(|said| said.text), calls)
}

#[test]
fn a_working_subagents_record_keeps_what_it_has_done_lately_and_lets_go_of_it_when_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("agent"));
    record(&store, &started("other"));
    // The prompt the CLI sends a subagent run in the foreground, which its call already shows.
    let prompt = EventBody::AgentUser { subagent: Some("agent".into()), blocks: vec![said("Find it.")] };
    assert_eq!(record(&store, &prompt).1, Vec::<String>::new(), "what it was asked isn't what it wrote");

    assert_eq!(record(&store, &message(Some("agent"), vec![said("I'll read it."), reading("read")])).1, ["agent"]);
    assert_eq!(record(&store, &answer(Some("agent"), "read", "1→fn main() {}")).1, ["agent"], "what came back");
    assert_eq!(record(&store, &answer(None, "read", "the agent's own")).1, Vec::<String>::new());
    assert_eq!(record(&store, &message(Some("agent"), vec![said("It's there.")])).1, ["agent"], "what it said");
    assert_eq!(record(&store, &message(Some("other"), vec![reading("grep")])).1, ["other"]);

    let made = subagent(&store, "agent").lately.unwrap();
    assert_eq!(made.calls[0].before.as_ref().map(|said| said.text.as_str()), Some("I'll read it."));
    assert_eq!(
        lately(&store, "agent"),
        (Some("It's there.".into()), vec![("read".into(), Some("1→fn main() {}".into()))])
    );

    record(&store, &ended("agent", TaskOutcome::Finished, Some("Found it.")));
    let finished = subagent(&store, "agent");
    assert_eq!((finished.lately, finished.calls), (None, 1), "it's only shown while it works");
    assert_eq!(record(&store, &message(Some("agent"), vec![said("Late.")])).1, Vec::<String>::new());
    assert_eq!(record(&store, &message(Some("agent"), vec![reading("late")])).1, ["agent"], "a call it made late");
    let late = subagent(&store, "agent");
    assert_eq!((late.lately, late.calls), (None, 2), "counted, but not shown");
    record(&store, &exited());
    assert_eq!(subagent(&store, "other").lately, None, "nor once it's gone with its agent");
}

#[test]
fn an_agent_that_exits_takes_the_subagents_still_working_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for body in [started("working"), started("done"), ended("done", TaskOutcome::Finished, Some("Done."))] {
        record(&store, &body);
    }
    let done = subagent(&store, "done");

    let (exit, changed) = record(&store, &exited());

    assert_eq!(changed, ["working"]);
    let stopped = subagent(&store, "working");
    assert_eq!((stopped.status, stopped.ended_at_ms, stopped.report), (Stopped, Some(exit.ts_ms), None));
    assert_eq!(subagent(&store, "done"), done, "it had ended already");
    assert_eq!(record(&store, &exited()).1, Vec::<String>::new());
}

#[test]
fn a_sessions_subagents_are_listed_oldest_first_as_of_a_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut other = store.get_session(&"s".into()).unwrap();
    other.session.id = "t".into();
    store.insert_session(&other, None).unwrap();
    record(&store, &started("b"));
    record(&store, &started("a"));
    store.append(&"t".into(), None, &started("c"), None, None, None).unwrap();

    let (subagents, rev) = store.list_subagents(&"s".into()).unwrap();

    let calls: Vec<&str> = subagents.iter().map(|subagent| subagent.call.as_str()).collect();
    assert_eq!(calls, ["b", "a"], "in the order they started, not by name");
    assert!(subagents.iter().all(|subagent| subagent.rev <= rev));
    assert_eq!(store.list_subagents(&"t".into()).unwrap().0.len(), 1);

    record(&store, &ended("a", TaskOutcome::Finished, None));
    assert!(subagent(&store, "a").rev > rev, "a change since is newer than the list");
}
