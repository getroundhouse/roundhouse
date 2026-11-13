use std::sync::Arc;

use client_core::Answered;
use client_core::session_log::{RecordedApprovals, SessionLog};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::{
    ApprovalOutcome, Asking, CallKind, ContentBlock, EventBody, Question, QuestionAnswer, SessionEvent,
};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::{
    CallResult, OpenApproval, PermissionMode, Place, RecentCall, RecentWork, Subagent, SubagentStatus,
};
use serde_json::json;

use super::{Answer, Around, Closes, Reply, Settled, block_key, retry_at};
use crate::fixtures::{
    asked, asking, calls, calls_to, closed, closed_by_position, event, failed_request, from_the_agent, log, prompt,
    record, result, turn_ended, within,
};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::{AtWork, Model};

#[test]
fn a_recorded_block_is_known_by_its_place_in_its_message() {
    let finished = |first_index| EventBody::Assistant {
        message_id: Some("m".into()),
        parent_tool_use_id: None,
        model: None,
        blocks: vec![ContentBlock::Text { text: "a".into(), images: Vec::new() }],
        first_index,
    };

    assert_eq!(block_key(7, &finished(Some(2)), 0), ItemKey::Block("m".into(), 2));
    assert_eq!(block_key(7, &finished(None), 0), ItemKey::Event(7, 0), "nothing streamed");
}

#[test]
fn a_question_is_drawn_only_once_it_s_no_longer_waiting() {
    let question = asked("q", "AskUserQuestion");
    let m = Model::default();
    let waiting = log([event(1, question.clone())]);
    assert!(Around::read(&waiting, &m, 1, &question).asked.is_none(), "it's in the composer's place");

    let answered = log([event(1, question.clone()), event(2, closed("q", ApprovalOutcome::Denied, None))]);

    assert!(Around::read(&answered, &m, 1, &question).asked.is_some());
    assert!(Around::read(&answered, &m, 1, &asked("a", "Bash")).asked.is_none(), "only a question is a line");
}

/// The transcript goes by what the adapter says a request asks, whatever its tool is called.
#[test]
fn a_request_its_adapter_says_asks_questions_is_drawn_as_one() {
    let question = Question { text: "Red or blue?".into(), header: None, options: Vec::new(), multiple: false };
    let questions = asking("q", Asking::Questions { questions: vec![question.clone()] });
    let tool = asking("t", Asking::ToolUse { call: None });
    let answered = closed_by_position("q", vec![QuestionAnswer { picked: Vec::new(), typed: Some("Blue".into()) }]);
    let log = log([event(1, questions.clone()), event(2, answered.clone()), event(3, tool.clone())]);
    let m = Model::default();

    let asked = Around::read(&log, &m, 1, &questions).asked.unwrap();

    assert_eq!(asked.questions, [question]);
    assert_eq!(
        asked.settled,
        Settled::Answered(Answered::ByPosition(vec![QuestionAnswer {
            picked: Vec::new(),
            typed: Some("Blue".into())
        }]))
    );
    assert!(matches!(Around::read(&log, &m, 2, &answered).closes, Closes::SaidElsewhere));
    assert!(Around::read(&log, &m, 3, &tool).asked.is_none(), "a tool's use is a card");
}

/// A command the agent asked to run, as the call `call`, and for the subagent `asked_by` if it's a subagent's.
fn asked_to_run(id: &str, call: &str, asked_by: Option<&str>) -> EventBody {
    let input = json!({ "command": "rm -rf build" });
    let asking = Some(Asking::ToolUse { call: Some(CallKind::Command) });
    let (tool_use_id, asked_by) = (Some(call.into()), asked_by.map(Into::into));
    ApprovalRequest { tool_use_id, input, asking, asked_by, ..approval_request(id, "Bash") }.into()
}

/// The result of the call `id`, which the person denied.
fn denied(id: &str) -> EventBody {
    EventBody::AgentUser {
        parent_tool_use_id: None,
        blocks: vec![protocol::fixtures::tool_denied(id, "The user denied this.", None)],
    }
}

#[test]
fn what_else_says_what_became_of_an_approval_is_a_questions_line_a_denied_calls_result_or_nothing() {
    let closing = |id: &str| closed(id, ApprovalOutcome::Denied, None);
    let log = log([
        event(1, asked("q", "AskUserQuestion")),
        event(2, closing("q")),
        event(3, asked_to_run("c", "run-c", None)),
        event(4, closing("c")),
        event(5, denied("run-c")),
        event(6, asked_to_run("old", "run-old", None)),
        event(7, closing("old")),
        event(8, result("run-old")),
        event(9, asked_to_run("s", "run-s", Some("agent"))),
        event(10, closing("s")),
        event(11, asked_to_run("x", "run-x", None)),
        event(12, closing("x")),
        event(13, denied("run-y")),
        event(14, asked_to_run("z", "run-z", None)),
        event(15, closed("z", ApprovalOutcome::Cancelled, None)),
        event(16, denied("run-z")),
    ]);
    let mut m = Model::default();
    let closes = |m: &Model, seq, id: &str| Around::read(&log, m, seq, &closing(id)).closes;

    assert!(matches!(closes(&m, 2, "q"), Closes::SaidElsewhere), "its line says it wasn't answered");
    assert!(matches!(closes(&m, 4, "c"), Closes::SaidElsewhere), "its call's result says it was denied");
    assert!(matches!(closes(&m, 7, "old"), Closes::Unsaid), "a result from before adapters said doesn't");
    assert!(matches!(closes(&m, 12, "x"), Closes::Unsaid), "nor does another call's");
    let cancelled = Around::read(&log, &m, 15, &closed("z", ApprovalOutcome::Cancelled, None)).closes;
    assert_eq!(cancelled, Closes::Unsaid, "nor a cancelled one's, which says only what a denial would");
    let for_a = |subagent: &str| Closes::ForASubagent { tool: "Bash".into(), subagent: subagent.into() };
    assert_eq!(closes(&m, 10, "s"), for_a("Subagent"), "its call is in the subagent's work");
    let checking = Subagent { description: Some("Check the build".into()), ..record("agent", SubagentStatus::Working) };
    m.subagents.insert("agent".into(), Arc::new(checking));
    assert_eq!(closes(&m, 10, "s"), for_a("Check the build"));
}

/// What's loaded can't say whether an approval's answer is in a page that isn't, and the session's record can.
#[test]
fn an_approvals_card_is_open_as_the_record_says_whatever_is_loaded_after_it() {
    let request = asked("a", "Bash");
    let with_a_gap = log([event(1, request.clone()), event(3, asked("b", "Bash"))]);
    let open = vec![OpenApproval { seq: 1, approval_id: "a".into() }, OpenApproval { seq: 3, approval_id: "b".into() }];
    let recorded = Model { approvals: Some(RecordedApprovals { open, through: 3 }), ..Model::default() };

    assert!(!Around::read(&with_a_gap, &Model::default(), 1, &request).approval_open, "from a hub that doesn't say");
    assert!(Around::read(&with_a_gap, &recorded, 1, &request).approval_open);
}

#[test]
fn calls_made_at_once_each_get_their_own_result() {
    let log = log([event(1, calls(&["a", "b"])), event(2, result("b")), event(3, result("a"))]);
    let m = Model::default();

    let around = Around::read(&log, &m, 1, &calls(&["a", "b"]));

    let found: Vec<Option<String>> =
        around.blocks.iter().map(|b| b.reply.result().map(|a| a.shown().to_string())).collect();
    assert_eq!(found, [Some("from a".to_string()), Some("from b".to_string())]);
    assert!(Around::read(&log, &m, 2, &result("b")).blocks[0].with_call);
    assert!(Around::read(&log, &m, 3, &result("a")).blocks[0].with_call);
}

/// The result of the call `id`, with an image.
fn with_an_image(id: &str) -> EventBody {
    let mut shown = result(id);
    if let EventBody::AgentUser { blocks, .. } = &mut shown
        && let [ContentBlock::ToolResult { images, .. }] = blocks.as_mut_slice()
    {
        let size_bytes = 1;
        images.push(Attachment { id: "i1".into(), name: "image".into(), media_type: MediaType::Png, size_bytes });
    }
    shown
}

#[test]
fn a_call_s_images_are_fetched_only_once_it_s_open() {
    let log = log([event(1, calls(&["a"])), event(2, with_an_image("a"))]);
    let mut m = Model::default();
    assert!(Around::read(&log, &m, 1, &calls(&["a"])).images().is_empty(), "closed");

    m.folds.choose(ItemKey::Event(1, 0), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, 1, &calls(&["a"])).images().len(), 1, "open");
}

#[test]
fn a_subagent_s_images_are_fetched_with_the_call_that_started_it() {
    let agent = calls_to("Agent", &["a"]);
    let log =
        log([event(1, agent.clone()), event(2, within("a", calls(&["b"]))), event(3, within("a", with_an_image("b")))]);
    let mut m = Model::default();
    m.folds.choose(ItemKey::Event(2, 0), Fold::Output, true);
    assert!(Around::read(&log, &m, 1, &agent).images().is_empty(), "the agent's call is closed");

    m.folds.choose(ItemKey::Event(1, 0), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, 1, &agent).images().len(), 1, "opened, it gathers its subagent's");
}

/// A chat that reads by place holds only the agent's own events, so what a working subagent has done lately comes
/// from its record, images and all.
#[test]
fn a_working_subagent_s_latest_images_are_fetched_once_its_call_under_working_on_is_open() {
    let agent = calls_to("Agent", &["agent"]);
    let mut log = SessionLog::by_place(Place::default());
    log.apply_event(SessionEvent { place: Place::new(1), ..event(1, agent.clone()) });
    let image = Attachment { id: "i1".into(), name: "image".into(), media_type: MediaType::Png, size_bytes: 1 };
    let read = protocol::fixtures::call("read", "Read", json!({ "file_path": "a.png" }));
    let came_back = CallResult { images: vec![image], ..protocol::fixtures::call_result("") };
    let recent =
        RecentWork { calls: vec![RecentCall { before: None, call: read, result: Some(came_back) }], since: None };
    let mut m = Model::default();
    m.subagents.insert("agent".into(), Arc::new(working("agent", recent)));
    m.folds.choose(ItemKey::Event(1, 0), Fold::Output, true);
    assert!(Around::read(&log, &m, 1, &agent).images().is_empty(), "its read is closed");

    m.folds.choose(ItemKey::Call("read".into()), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, 1, &agent).images().len(), 1, "open, under Working on");
}

fn working(call: &str, recent: RecentWork) -> Subagent {
    Subagent { calls: 1, recent, ..record(call, SubagentStatus::Working) }
}

#[test]
fn a_result_from_the_agent_is_its_call_s_result_too() {
    let sent = from_the_agent(result("a"));
    let log = log([event(1, calls_to("advisor", &["a"])), event(2, sent.clone())]);
    let m = Model::default();

    let call = Around::read(&log, &m, 1, &calls_to("advisor", &["a"]));

    assert_eq!(call.blocks[0].reply.result().map(Answer::shown), Some("from a"));
    assert!(Around::read(&log, &m, 2, &sent).blocks[0].with_call, "so it isn't drawn twice");
}

#[test]
fn a_call_the_log_says_was_dropped_has_no_result_unless_one_came_after_all() {
    let dropped = EventBody::ToolCallDropped { tool_use_id: "a".into() };
    let gone = log([event(1, calls(&["a", "b"])), event(2, dropped.clone())]);
    let came = log([event(1, calls(&["a"])), event(2, dropped), event(3, result("a"))]);
    let m = Model::default();

    let beside = Around::read(&gone, &m, 1, &calls(&["a", "b"]));
    let late = Around::read(&came, &m, 1, &calls(&["a"]));

    assert!(matches!(beside.blocks[0].reply, Reply::Dropped));
    assert!(matches!(beside.blocks[1].reply, Reply::Nothing), "the call made beside it is still waiting");
    assert_eq!(late.blocks[0].reply.result().map(Answer::shown), Some("from a"), "it stands");
}

#[test]
fn a_change_of_mode_knows_the_mode_it_changed_from() {
    let changed_to = |mode| EventBody::AgentMode { permission_mode: mode };
    let log = log([event(1, changed_to(PermissionMode::Plan)), event(2, changed_to(PermissionMode::FullAccess))]);

    let around = Around::read(&log, &Model::default(), 2, &changed_to(PermissionMode::FullAccess));

    assert_eq!(around.mode_before, Some(PermissionMode::Plan));
}

#[test]
fn a_result_without_its_call_nearby_is_drawn_where_it_is() {
    let log = log([event(1, calls(&["a"])), event(2, result("elsewhere"))]);
    let m = Model::default();

    assert!(!Around::read(&log, &m, 2, &result("elsewhere")).blocks[0].with_call);
    let call = Around::read(&log, &m, 1, &calls(&["a"]));
    assert!(
        matches!(call.blocks[0].reply, Reply::Nothing),
        "a call whose result hasn't come back has nothing under it"
    );
}

/// A failed result is read as it was recorded: one that says what kind of call it answers says how the command exited
/// beside its text, and one from before has it in its text.
#[test]
fn a_failed_result_says_how_its_command_exited_however_it_was_recorded() {
    let before = protocol::fixtures::tool_result("c", "Exit code 2\nno rule to make it", true);
    let since = protocol::fixtures::command_failed("c", "no rule to make it", 2);

    for block in [before, since] {
        let reply = Reply::of(&block, &Model::default());
        let failure = reply.result().and_then(Answer::failure).unwrap();
        assert_eq!((failure.label().as_str(), failure.output), ("Exit 2", "no rule to make it"));
    }
}

/// Retry asks the agent to carry on from the turn it stopped short on, so only the card that turn ended on has it, and
/// only while the agent is still stopped there.
#[test]
fn only_the_card_the_newest_turn_ended_on_offers_retry_until_something_is_sent() {
    let taken = |prompt| EventBody::PromptTaken { prompt };
    let turns = [
        event(1, prompt("Fix it")),
        event(2, taken(1)),
        event(3, failed_request(None)),
        event(4, turn_ended()),
        event(5, prompt("Continue")),
        event(6, taken(5)),
        event(7, failed_request(None)),
        event(8, turn_ended()),
    ];
    let twice = log(turns.clone());
    let mut at_work = AtWork { reply_seq: 8, ..AtWork::default() };
    assert_eq!(retry_at(&twice, &at_work), None, "the record doesn't say the turn failed");

    at_work.failed = true;

    assert_eq!(retry_at(&twice, &at_work), Some(7), "not the older turn's");
    let sent = log(turns.into_iter().chain([event(9, prompt("Try Sonnet"))]));
    assert_eq!(retry_at(&sent, &at_work), None, "something has been sent since");
}
