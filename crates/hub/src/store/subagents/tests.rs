use protocol::event::{ExitReason, SubagentOutcome};
use protocol::model::SubagentStatus::{Finished, Stopped, Working};
use serde_json::json;

use super::*;
use crate::store::tests::store_with_session;

fn started(call: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        kind: Some("Explore".into()),
        description: Some("Find the tests".into()),
        prompt: Some("Find where the tests live.".into()),
        in_background: false,
    }
}

fn ended(call: &str, outcome: SubagentOutcome, report: Option<&str>) -> EventBody {
    EventBody::SubagentEnded { call: call.into(), outcome, report: report.map(Into::into), context_tokens: None }
}

fn tool_use(id: &str) -> ContentBlock {
    ContentBlock::ToolUse { id: id.into(), name: "Read".into(), input: json!({"file_path": id}), call: None }
}

/// A message from the agent, or from the subagent `from` started.
fn message(from: Option<&str>, blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: from.map(Into::into),
        model: None,
        blocks,
        first_index: None,
    }
}

/// A reply from `model`, from the agent, or from the subagent `from` started.
fn reply(from: Option<&str>, model: &str, blocks: Vec<ContentBlock>) -> EventBody {
    let mut reply = message(from, blocks);
    if let EventBody::Assistant { model: named, .. } = &mut reply {
        *named = Some(model.into());
    }
    reply
}

fn model(store: &Store, call: &str) -> Option<String> {
    subagent(store, call).model.map(String::from)
}

fn other(kind: &str) -> EventBody {
    EventBody::AgentOther { kind: kind.into(), parent_tool_use_id: None }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] }
}

/// Records `body` in the session `s`, and says which subagents' records it changed.
fn record(store: &Store, body: &EventBody) -> (SessionEvent, Vec<String>) {
    let appended = store.append(&"s".into(), None, body, None, None, None).unwrap();
    (appended.event, appended.subagents.iter().map(|call| call.as_str().to_owned()).collect())
}

fn subagent(store: &Store, call: &str) -> Subagent {
    store.get_subagent(&"s".into(), &call.into()).unwrap().unwrap()
}

#[test]
fn a_subagents_record_starts_with_what_it_was_asked_counts_its_calls_and_ends_with_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let (start, changed) = record(&store, &started("agent"));
    assert_eq!(changed, ["agent"]);
    let first = subagent(&store, "agent");
    assert_eq!((first.status, first.started_at_ms, first.calls, &first.latest_call), (Working, start.ts_ms, 0, &None));
    assert_eq!((first.kind.as_deref(), first.prompt.as_deref()), (Some("Explore"), Some("Find where the tests live.")));

    assert_eq!(record(&store, &message(None, vec![tool_use("own")])).1, Vec::<String>::new(), "the agent's own");
    assert_eq!(record(&store, &message(Some("other"), vec![tool_use("x")])).1, Vec::<String>::new(), "no record");
    let text = vec![ContentBlock::Thinking { text: "x".into() }];
    assert_eq!(record(&store, &message(Some("agent"), text)).1, Vec::<String>::new(), "nothing it shows");
    let blocks =
        vec![tool_use("read-1"), ContentBlock::Text { text: "x".into(), images: Vec::new() }, tool_use("read-2")];
    assert_eq!(record(&store, &message(Some("agent"), blocks)).1, ["agent"]);
    let working = subagent(&store, "agent");
    assert_eq!(working.calls, 2);
    assert_eq!(working.latest_call.as_ref().map(|call| call.id.as_str()), Some("read-2"), "the newest");
    assert_eq!(working.latest_call.unwrap().input, json!({"file_path": "read-2"}));
    assert!(working.rev > first.rev, "a client only takes a newer copy");

    let (end, changed) = record(&store, &ended("agent", SubagentOutcome::Finished, Some("They're in tests/.")));
    assert_eq!(changed, ["agent"]);
    let finished = subagent(&store, "agent");
    assert_eq!((finished.status, finished.ended_at_ms), (Finished, Some(end.ts_ms)));
    assert_eq!((finished.calls, finished.report.as_deref()), (2, Some("They're in tests/.")));
    assert!(finished.rev > working.rev);

    assert_eq!(record(&store, &ended("agent", SubagentOutcome::Failed, None)).1, Vec::<String>::new(), "ended once");
    assert_eq!(record(&store, &started("agent")).1, Vec::<String>::new(), "started once");
    assert_eq!(subagent(&store, "agent"), finished);
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

    assert_eq!(record(&store, &reply(Some("agent"), "claude-haiku-5-5", thinking())).1, unchanged, "the same model");
    assert_eq!(record(&store, &message(Some("agent"), thinking())).1, unchanged, "a reply that names none");
    assert_eq!(record(&store, &reply(Some("agent"), "not a model", thinking())).1, unchanged, "a name that names none");
    assert_eq!(record(&store, &reply(None, "claude-opus-5-5", thinking())).1, unchanged, "the agent's own");
    assert_eq!(subagent(&store, "agent"), on_haiku, "it keeps its revision, so it isn't sent again");

    let (_, changed) = record(&store, &reply(Some("agent"), "claude-sonnet-5-5", vec![tool_use("read")]));
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
    let holds = |call: &str, tokens| EventBody::SubagentContext { call: call.into(), tokens };
    let unchanged = Vec::<String>::new();

    assert_eq!(subagent(&store, "agent").context_tokens, None, "before the agent says");
    assert_eq!(record(&store, &holds("agent", 1_200)).1, ["agent"]);
    let counted = subagent(&store, "agent");
    assert_eq!(counted.context_tokens, Some(1_200));

    assert_eq!(record(&store, &holds("agent", 1_200)).1, unchanged, "the same count");
    assert_eq!(record(&store, &holds("nobody", 7)).1, unchanged, "one the agent never said it started");
    assert_eq!(subagent(&store, "agent"), counted, "it keeps its revision, so it isn't sent again");
    assert_eq!(record(&store, &holds("agent", 900)).1, ["agent"]);
    assert_eq!(subagent(&store, "agent").context_tokens, Some(900), "the newest count, which can be less");
    record(&store, &holds("agent", u64::MAX));
    assert_eq!(subagent(&store, "agent").context_tokens, Some(u64::MAX), "however large the agent says");

    record(&store, &holds("agent", 900));
    assert_eq!(record(&store, &ended("agent", SubagentOutcome::Stopped, None)).1, ["agent"]);
    assert_eq!(subagent(&store, "agent").context_tokens, Some(900), "an end that counts none leaves its count");
    assert_eq!(record(&store, &holds("agent", 5_000)).1, unchanged, "it's ended, and its end has the last word");

    record(&store, &started("other"));
    record(&store, &holds("other", 10));
    let end = EventBody::SubagentEnded {
        call: "other".into(),
        outcome: SubagentOutcome::Finished,
        report: None,
        context_tokens: Some(2_000),
    };
    record(&store, &end);
    assert_eq!(subagent(&store, "other").context_tokens, Some(2_000), "the count as it ended");
}

fn said(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

/// What came back for the call `id`, to the agent, or to the subagent `to` started.
fn answer(to: Option<&str>, id: &str, text: &str) -> EventBody {
    let result = ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error: false,
        text: text.into(),
        images: Vec::new(),
        change: None,
        encrypted: false,
        call: None,
        exit_code: None,
    };
    EventBody::AgentUser { parent_tool_use_id: to.map(Into::into), blocks: vec![result] }
}

/// What the subagent has said since its newest call, and each of its calls kept, with what came back for it.
fn recent(store: &Store, call: &str) -> (Option<String>, Vec<(String, Option<String>)>) {
    let work = subagent(store, call).recent;
    let calls = work.calls.into_iter().map(|made| (made.call.id.to_string(), made.result.map(|r| r.text))).collect();
    (work.since.map(|said| said.text), calls)
}

#[test]
fn a_working_subagents_record_keeps_what_it_has_done_lately_and_lets_go_of_it_when_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("agent"));
    record(&store, &started("other"));
    // The prompt the CLI sends a subagent run in the foreground, which its call already shows.
    let prompt = EventBody::AgentUser { parent_tool_use_id: Some("agent".into()), blocks: vec![said("Find it.")] };
    assert_eq!(record(&store, &prompt).1, Vec::<String>::new(), "what it was asked isn't what it wrote");

    assert_eq!(record(&store, &message(Some("agent"), vec![said("I'll read it."), tool_use("read")])).1, ["agent"]);
    assert_eq!(record(&store, &answer(Some("agent"), "read", "1→fn main() {}")).1, ["agent"], "what came back");
    assert_eq!(record(&store, &answer(None, "read", "the agent's own")).1, Vec::<String>::new());
    assert_eq!(record(&store, &message(Some("agent"), vec![said("It's there.")])).1, ["agent"], "what it said");
    assert_eq!(record(&store, &message(Some("other"), vec![tool_use("grep")])).1, ["other"]);

    let made = subagent(&store, "agent");
    assert_eq!(made.recent.calls[0].before.as_ref().map(|said| said.text.as_str()), Some("I'll read it."));
    assert_eq!(
        recent(&store, "agent"),
        (Some("It's there.".into()), vec![("read".into(), Some("1→fn main() {}".into()))])
    );
    assert_eq!(made.latest_call.as_ref().map(|call| call.id.as_str()), Some("read"), "for a client from before");

    record(&store, &ended("agent", SubagentOutcome::Finished, Some("Found it.")));
    let finished = subagent(&store, "agent");
    assert_eq!(finished.recent, RecentWork::default(), "it's only shown while it works");
    assert_eq!(finished.latest_call, made.latest_call);
    assert_eq!(record(&store, &message(Some("agent"), vec![said("Late.")])).1, Vec::<String>::new());
    record(&store, &exited());
    assert_eq!(subagent(&store, "other").recent, RecentWork::default(), "nor once it's gone with its agent");
}

#[test]
fn an_agent_that_exits_takes_the_subagents_still_working_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for body in [started("working"), started("done"), ended("done", SubagentOutcome::Finished, Some("Done."))] {
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

    record(&store, &ended("a", SubagentOutcome::Finished, None));
    assert!(subagent(&store, "a").rev > rev, "a change since is newer than the list");
}

mod migrated;
