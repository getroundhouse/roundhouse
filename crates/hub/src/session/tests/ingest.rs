//! What the session takes in from its agent's worker: what it tells the feed, what it has heard already, and what the
//! hub answers for the person.

use super::super::ingest::{heard_before, says_something_new, the_hub_allows};
use super::super::*;

fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

#[test]
fn only_the_run_going_says_how_full_the_context_is_and_only_a_change_counts() {
    let (going, replaced) = (RunId::new(2), RunId::new(1));

    assert!(says_something_new(Some(going), None, going, &context(1)), "the first report");
    assert!(says_something_new(Some(going), Some(&context(1)), going, &context(2)));
    assert!(
        !says_something_new(Some(going), Some(&context(1)), going, &context(1)),
        "the feed isn't told what it knows"
    );
    assert!(!says_something_new(Some(going), Some(&context(1)), replaced, &context(2)), "a replaced run's");
    assert!(!says_something_new(None, Some(&context(1)), going, &context(2)), "a run that has exited");
}

/// As a worker sends a hub that connects to it what it sent the last hub, which may have recorded some of it.
#[test]
fn what_a_worker_sends_again_is_taken_in_once() {
    let (first, next) = (RunId::new(3), RunId::new(4));
    let mut recorded = Some((first, 5));
    let heard: Vec<_> = [4, 5, 6, 7].into_iter().map(|seq| heard_before(&mut recorded, first, seq)).collect();
    assert_eq!(heard, [true, true, false, false], "only what came after the last recorded");
    assert_eq!(recorded, Some((first, 7)));
    assert!(heard_before(&mut recorded, first, 7), "and each only once");
    assert!(!heard_before(&mut recorded, next, 2), "another run's is never the same, whatever its number");
    assert_eq!(recorded, Some((next, 2)));
    let mut never = None;
    assert!(!heard_before(&mut never, first, 1), "nothing is before the first");
}

#[test]
fn the_hub_allows_only_a_tools_use_asked_about_with_full_access_and_only_when_set_to() {
    use protocol::capability::approvals::{Asks, Request};
    use protocol::capability::calls::Call;
    use protocol::event::{Asking, CallKind};
    use protocol::model::PermissionMode::{FullAccess, Plan, Unknown};

    let asked = |asking: Option<Asking>| {
        let mut body: EventBody = protocol::fixtures::approval_request("a", "Bash").into();
        let EventBody::ApprovalRequested { asking: kept, .. } = &mut body else { unreachable!() };
        *kept = asking.map(Box::new);
        body
    };
    let typed = |asks: Asks, always: bool| {
        let mut body = asked(None);
        let EventBody::ApprovalRequested { request, .. } = &mut body else { unreachable!() };
        *request = Some(Box::new(Request { asks, suggested: Vec::new(), always, why: None }));
        body
    };
    let tool = asked(Some(Asking::ToolUse { call: Some(CallKind::Command) }));
    let call = || Asks::Call { call: Call::Other { tool: "t".into(), input: serde_json::Value::Null } };
    assert!(the_hub_allows(FullAccess, &tool, true));
    assert!(the_hub_allows(FullAccess, &asked(Some(Asking::ToolUse { call: None })), true), "whatever kind of call");
    for always in [true, false] {
        assert!(the_hub_allows(FullAccess, &typed(call(), always), true), "a typed call, asked always or not");
    }
    assert!(!the_hub_allows(FullAccess, &tool, false), "only when set to");
    assert!(!the_hub_allows(FullAccess, &typed(call(), true), false), "a typed one only when set to");
    for mode in [Plan, Unknown] {
        assert!(!the_hub_allows(mode, &tool, true), "{mode:?} asks the person");
        assert!(!the_hub_allows(mode, &typed(call(), true), true), "{mode:?} asks the person of a typed one");
    }
    let questions = Asking::Questions { questions: Vec::new() };
    let plan = Asking::Plan { plan: "Do it".into() };
    for asking in [Some(questions), Some(plan), Some(Asking::Unknown), None] {
        assert!(!the_hub_allows(FullAccess, &asked(asking.clone()), true), "{asking:?} is the person's to answer");
    }
    for asks in
        [Asks::Questions { questions: Vec::new() }, Asks::Plan { plan: "Do it".into(), file: None }, Asks::Unknown]
    {
        assert!(!the_hub_allows(FullAccess, &typed(asks.clone(), true), true), "{asks:?} is the person's to answer");
    }
}
