use serde_json::json;

use super::*;
use crate::model::{CacheTtl, WaitUntil};

/// Waits are the hub's, which the next hub keeps, starting the agent again once one ends, so an agent with only
/// waits in the background isn't counted as running. One with anything else there, or on a turn, is.
#[test]
fn an_agent_with_only_waits_in_the_background_isnt_counted_as_running() {
    use SessionStatus::{Background, Idle, Running, WaitingApproval};
    let task = |id: &str, kind| AgentTask { id: id.into(), description: None, kind, call: None };
    let (wait, other_wait, command) =
        (task("w", TaskKind::Wait), task("x", TaskKind::Wait), task("c", TaskKind::Command));

    assert!(!Background.counts_as_running(std::slice::from_ref(&wait)));
    assert!(!Background.counts_as_running(&[wait.clone(), other_wait]));
    assert!(Background.counts_as_running(&[wait.clone(), command.clone()]));
    assert!(Background.counts_as_running(&[command]));
    assert!(Background.counts_as_running(&[]), "nothing says it only waits");
    for in_a_turn in [Running, WaitingApproval] {
        assert!(in_a_turn.counts_as_running(std::slice::from_ref(&wait)), "{in_a_turn:?}");
    }
    assert!(!Idle.counts_as_running(&[]));
}

/// A chat in `status`, as a hub names it, whose agent's context holds `used_tokens`.
fn session(status: &str, used_tokens: u64) -> Session {
    let context =
        ContextUsage { used_tokens, max_tokens: 1_000_000, model_tokens: None, max_set_by: None, parts: Vec::new() };
    Session {
        status: serde_json::from_value(json!(status)).unwrap(),
        last_seq: Seq::new(4),
        context: Some(context),
        prompt_cache: Some(PromptCache { used_at_ms: 0, ttl: CacheTtl::OneHour, model: None, tokens: 0 }),
        ..crate::fixtures::session("s")
    }
}

/// A hub that doesn't keep waits sends none. One that does sends the open one in this form, which its store's query
/// writes, with a `call` the CLI didn't say as null.
#[test]
fn a_session_from_a_hub_that_doesnt_keep_waits_has_none_and_one_that_does_has_its_open_wait() {
    let mut said = serde_json::to_value(session("background", 0)).unwrap();
    said["wait"] = json!({
        "id": "a", "session_id": "s", "workspace_id": "w", "number": 7, "call": null, "until": "checks",
        "started_at_ms": 1,
    });

    let waiting: Session = serde_json::from_value(said).unwrap();

    assert_eq!(session("idle", 0).wait, None);
    let wait = waiting.wait.unwrap();
    assert_eq!((wait.number, wait.call, wait.until, wait.ended), (7, None, WaitUntil::Checks, None));
}

#[test]
fn a_cache_is_worth_telling_of_once_the_context_is_big_enough_and_the_agent_isnt_working() {
    assert!(session("idle", CACHE_WORTH_TELLING_TOKENS).cache_worth_telling().is_some());
    assert!(session("waiting_approval", 313_500).cache_worth_telling().is_some(), "it can expire while it waits");
    assert!(session("idle", CACHE_WORTH_TELLING_TOKENS - 1).cache_worth_telling().is_none());
    assert!(session("running", 313_500).cache_worth_telling().is_none(), "each request keeps it warm");
    let closed = Session { closed_at_ms: Some(1), ..session("idle", 313_500) };
    assert!(closed.cache_worth_telling().is_none(), "nobody will send a closed chat anything");
}

/// Each grouping, for every status. One from a newer peer is in none of them.
#[test]
fn each_status_is_in_the_groups_that_say_what_its_agent_is_doing() {
    use SessionStatus::{Background, Idle, Interrupted, Running, Unknown, WaitingApproval};
    let groups = |status: SessionStatus| {
        [status.in_turn(), status.is_active(), status.is_working(), status.between_turns(), status.is_done()]
    };

    // In a turn, active, working, between turns, done.
    assert_eq!(groups(Idle), [false, false, false, true, true]);
    assert_eq!(groups(Running), [true, true, true, false, false]);
    assert_eq!(groups(WaitingApproval), [true, true, false, false, false]);
    assert_eq!(groups(Interrupted), [false, false, false, false, true]);
    assert_eq!(groups(Background), [false, true, false, true, false]);
    assert_eq!(groups(Unknown), [false; 5]);
}

#[test]
fn the_turns_card_says_what_the_cache_holds_whatever_the_size_but_not_while_the_agent_works() {
    assert!(session("idle", 9_000).idle_cache().is_some());
    assert!(session("running", 9_000).idle_cache().is_none());
    assert!(session("thinking_hard", 9_000).idle_cache().is_none(), "a status from a newer hub might be working");
}

/// As the hub refuses one, so the button to ask is off when asking would be refused.
#[test]
fn a_compaction_waits_for_the_turn_for_every_prompt_to_be_taken_and_for_a_conversation() {
    let says = |status, untaken: &[Seq], agent_opened| Session {
        untaken: Some(untaken.to_vec()),
        agent_opened: Some(agent_opened),
        ..session(status, 9_000)
    };
    assert!(!says("idle", &[], true).compaction_waits());
    assert!(says("running", &[], true).compaction_waits());
    assert!(says("idle", &[Seq::new(5)], true).compaction_waits(), "a prompt or compaction on its way");
    assert!(says("idle", &[], false).compaction_waits(), "nothing to compact yet");
    assert!(session("running", 9_000).compaction_waits(), "a hub that doesn't say still says when it's working");
    assert!(!session("idle", 9_000).compaction_waits(), "and is asked otherwise");
}

/// Either way the agent stopped short lasts until a turn starts again, and a turn that started since a failed one says
/// nothing of how that one ended until it ends itself.
#[test]
fn the_agent_stops_short_by_going_away_mid_turn_or_by_failing_a_turn() {
    use SessionStatus::{Background, Idle, Interrupted, Running, WaitingApproval};
    let chat = |status, reply_outcome| Session { status, reply_outcome, ..crate::fixtures::session("s") };

    assert!(chat(Interrupted, None).stopped_short());
    assert!(chat(Interrupted, Some(TurnOutcome::Success)).stopped_short());
    assert!(chat(Idle, Some(TurnOutcome::Error)).stopped_short());
    assert!(chat(Background, Some(TurnOutcome::Error)).stopped_short(), "with work left going in the background");

    for started_again in [Running, WaitingApproval] {
        assert!(!chat(started_again, Some(TurnOutcome::Error)).stopped_short(), "{started_again:?}");
    }
    for ended in [None, Some(TurnOutcome::Success), Some(TurnOutcome::Cancelled), Some(TurnOutcome::Unknown)] {
        assert!(!chat(Idle, ended).stopped_short(), "{ended:?}");
    }
    assert!(!chat(SessionStatus::Unknown, Some(TurnOutcome::Error)).stopped_short(), "claims nothing it can't tell");
}

#[test]
fn a_chat_has_a_reply_once_a_turn_has_ended_whether_or_not_it_s_read() {
    let new = crate::fixtures::session("s");
    let replied = Session { reply_seq: Seq::FIRST, ..new.clone() };
    let read = Session { read_seq: Seq::FIRST, ..replied.clone() };

    assert_eq!([new.has_reply(), replied.has_reply(), read.has_reply()], [false, true, true]);
    assert_eq!([replied.unread(), read.unread()], [true, false]);
}
