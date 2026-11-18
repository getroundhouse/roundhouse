use std::sync::Arc;

use client_core::Answered;
use client_core::session_log::{RecordedApprovals, SessionLog};
use protocol::attachment::{Attachment, MediaType};
use protocol::capability::approvals::{Asks, Decision};
use protocol::capability::asking::QuestionAnswer;
use protocol::capability::calls::call::Read;
use protocol::capability::calls::output::Ended;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::fixtures::{approval_request, cancelled, decided, deny, question as asked_as};
use protocol::model::{Lately, LatelyCall, LatelyResult, OpenApproval, Place, Seq, Subagent, SubagentStatus};

use super::{Answer, Around, Closes, Reply, Settled, block_key, retry_at};
use crate::fixtures::{
    advised, asked_a_question, asked_to_use, asking, calls, event, failed_request, from_the_agent, log, prompt, record,
    result, subagents, turn_ended, within,
};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::{AtWork, Model};

#[test]
fn a_recorded_block_is_known_by_its_place_in_its_message() {
    let finished = |first_index| EventBody::Assistant {
        message_id: Some("m".into()),
        subagent: None,
        model: None,
        blocks: vec![ContentBlock::Text { text: "a".into(), images: Vec::new() }],
        first_index,
    };

    assert_eq!(block_key(Seq::new(7), &finished(Some(2)), 0), ItemKey::Block("m".into(), 2));
    assert_eq!(block_key(Seq::new(7), &finished(None), 0), ItemKey::Event(Seq::new(7), 0), "nothing streamed");
}

#[test]
fn a_question_is_drawn_only_once_it_s_no_longer_waiting() {
    let question = asked_a_question("q");
    let m = Model::default();
    let waiting = log([event(Seq::new(1), question.clone())]);
    assert!(Around::read(&waiting, &m, Seq::new(1), &question).asked.is_none(), "it's in the composer's place");

    let skipped = Decision::Answer { answers: vec![QuestionAnswer::default()] };
    let answered = log([event(Seq::new(1), question.clone()), event(Seq::new(2), decided("q", skipped))]);

    assert!(Around::read(&answered, &m, Seq::new(1), &question).asked.is_some());
    assert!(Around::read(&answered, &m, Seq::new(1), &asked_to_use("a")).asked.is_none(), "only a question is a line");
}

/// The transcript goes by what the adapter says a request asks.
#[test]
fn a_request_its_adapter_says_asks_questions_is_drawn_as_one() {
    let question = asked_as("Red or blue?", &[], false);
    let questions = asking("q", Asks::Questions { questions: vec![question.clone()] });
    let tool = asked_to_use("t");
    let blue = vec![QuestionAnswer { picked: Vec::new(), typed: Some("Blue".into()) }];
    let answered = decided("q", Decision::Answer { answers: blue.clone() });
    let log = log([
        event(Seq::new(1), questions.clone()),
        event(Seq::new(2), answered.clone()),
        event(Seq::new(3), tool.clone()),
    ]);
    let m = Model::default();

    let asked = Around::read(&log, &m, Seq::new(1), &questions).asked.unwrap();

    assert_eq!(asked.questions, [question]);
    assert_eq!(asked.settled, Settled::Answered(Answered(blue)));
    assert!(matches!(Around::read(&log, &m, Seq::new(2), &answered).closes, Closes::SaidElsewhere));
    assert!(Around::read(&log, &m, Seq::new(3), &tool).asked.is_none(), "a tool's use is a card");
}

/// A command the agent asked to run, as the call `call`, and for the subagent `asked_by` if it's a subagent's.
fn asked_to_run(id: &str, call: &str, asked_by: Option<&str>) -> EventBody {
    let call_to_run = Call::Command(protocol::fixtures::typed_command("rm -rf build"));
    let mut request = approval_request(id, Asks::Call { call: call_to_run });
    (request.tool_use_id, request.asked_by) = (Some(call.into()), asked_by.map(Into::into));
    request.into()
}

/// The result of the call `id`, which the person denied.
fn denied(id: &str) -> EventBody {
    EventBody::AgentUser {
        subagent: None,
        blocks: vec![protocol::fixtures::typed_denied(id, "The user denied this.", None)],
    }
}

#[test]
fn what_else_says_what_became_of_an_approval_is_a_questions_line_a_denied_calls_result_or_nothing() {
    let closing = |id: &str| decided(id, deny(None));
    let log = log([
        event(Seq::new(1), asked_a_question("q")),
        event(Seq::new(2), closing("q")),
        event(Seq::new(3), asked_to_run("c", "run-c", None)),
        event(Seq::new(4), closing("c")),
        event(Seq::new(5), denied("run-c")),
        event(Seq::new(6), asked_to_run("old", "run-old", None)),
        event(Seq::new(7), closing("old")),
        event(Seq::new(8), result("run-old")),
        event(Seq::new(9), asked_to_run("s", "run-s", Some("agent"))),
        event(Seq::new(10), closing("s")),
        event(Seq::new(11), asked_to_run("x", "run-x", None)),
        event(Seq::new(12), closing("x")),
        event(Seq::new(13), denied("run-y")),
        event(Seq::new(14), asked_to_run("z", "run-z", None)),
        event(Seq::new(15), cancelled("z")),
        event(Seq::new(16), denied("run-z")),
    ]);
    let mut m = Model::default();
    let closes = |m: &Model, seq, id: &str| Around::read(&log, m, seq, &closing(id)).closes;

    assert!(matches!(closes(&m, Seq::new(2), "q"), Closes::SaidElsewhere), "its line says it wasn't answered");
    assert!(matches!(closes(&m, Seq::new(4), "c"), Closes::SaidElsewhere), "its call's result says it was denied");
    assert!(matches!(closes(&m, Seq::new(7), "old"), Closes::Unsaid), "a result from before adapters said doesn't");
    assert!(matches!(closes(&m, Seq::new(12), "x"), Closes::Unsaid), "nor does another call's");
    let nobody = Around::read(&log, &m, Seq::new(15), &cancelled("z")).closes;
    assert_eq!(nobody, Closes::Unsaid, "nor a cancelled one's, which says only what a denial would");
    let for_a = |subagent: &str| Closes::ForASubagent { tool: "Command".into(), subagent: subagent.into() };
    assert_eq!(closes(&m, Seq::new(10), "s"), for_a("Subagent"), "its call is in the subagent's work");
    let checking = Subagent { description: Some("Check the build".into()), ..record("agent", SubagentStatus::Working) };
    m.subagents.insert(Arc::new(checking));
    assert_eq!(closes(&m, Seq::new(10), "s"), for_a("Check the build"));
}

/// A typed request names its call as Claude's does, and its call's typed result says it was denied, so its line says it
/// once.
#[test]
fn a_typed_calls_result_says_it_was_denied_too() {
    let command = protocol::capability::calls::Call::Command(protocol::fixtures::typed_command("rm -rf build"));
    let closing = decided("c", deny(None));
    let typed = |blocks| EventBody::AgentUser { subagent: None, blocks };
    let log = log([
        event(Seq::new(1), protocol::fixtures::typed_request("c", "run-c", command)),
        event(Seq::new(2), closing.clone()),
        event(Seq::new(3), typed(vec![protocol::fixtures::typed_denied("run-c", "The user denied this.", None)])),
    ]);

    let closes = Around::read(&log, &Model::default(), Seq::new(2), &closing).closes;

    assert!(matches!(closes, Closes::SaidElsewhere), "{closes:?}");
}

/// What a typed result holds reaches its call's row as one recorded as the agent's words does: the images that came
/// back beside it, that it's for the model alone, and that a command went on in the background.
#[test]
fn a_typed_result_answers_as_fully_as_any_other() {
    use protocol::capability::calls::CallOutput;
    use protocol::capability::calls::output::Ended;
    let m = Model::default();
    let done = protocol::fixtures::typed_result("c", CallOutput::Words { text: "done".into() }, false);
    let beside = protocol::fixtures::with_images(done, vec![protocol::fixtures::image("i")]);
    let backgrounded = Ended::Backgrounded { task: "t".into() };
    let answer = |block: &ContentBlock| {
        Reply::of(block, &m).result().map(|answer| (answer.images.len(), answer.encrypted, answer.in_the_background()))
    };

    assert_eq!(answer(&beside), Some((1, false, false)));
    assert_eq!(answer(&protocol::fixtures::typed_result("c", CallOutput::Encrypted, false)), Some((0, true, false)));
    let in_the_background =
        protocol::fixtures::typed_result("c", protocol::fixtures::command_output("", None, backgrounded), false);
    assert_eq!(answer(&in_the_background), Some((0, false, true)));
}

/// What's loaded can't say whether an approval's answer is in a page that isn't, and the session's record can.
#[test]
fn an_approvals_card_is_open_as_the_record_says_whatever_is_loaded_after_it() {
    let request = asked_to_use("a");
    let with_a_gap = log([event(Seq::new(1), request.clone()), event(Seq::new(3), asked_to_use("b"))]);
    let open = vec![
        OpenApproval { seq: Seq::new(1), approval_id: "a".into() },
        OpenApproval { seq: Seq::new(3), approval_id: "b".into() },
    ];
    let recorded = Model { approvals: Some(RecordedApprovals { open, through: Seq::new(3) }), ..Model::default() };

    assert!(
        !Around::read(&with_a_gap, &Model::default(), Seq::new(1), &request).approval_open,
        "from a hub that doesn't say"
    );
    assert!(Around::read(&with_a_gap, &recorded, Seq::new(1), &request).approval_open);
}

#[test]
fn calls_made_at_once_each_get_their_own_result() {
    let log =
        log([event(Seq::new(1), calls(&["a", "b"])), event(Seq::new(2), result("b")), event(Seq::new(3), result("a"))]);
    let m = Model::default();

    let around = Around::read(&log, &m, Seq::new(1), &calls(&["a", "b"]));

    let found: Vec<Option<String>> =
        around.blocks.iter().map(|b| b.reply.result().map(|a| a.shown().to_string())).collect();
    assert_eq!(found, [Some("from a".to_string()), Some("from b".to_string())]);
    assert!(Around::read(&log, &m, Seq::new(2), &result("b")).blocks[0].with_call);
    assert!(Around::read(&log, &m, Seq::new(3), &result("a")).blocks[0].with_call);
}

/// The result of the call `id`, with an image.
fn with_an_image(id: &str) -> EventBody {
    let mut shown = result(id);
    if let EventBody::AgentUser { blocks, .. } = &mut shown
        && let [ContentBlock::CallResult { images, .. }] = blocks.as_mut_slice()
    {
        let size_bytes = 1;
        images.push(Attachment { id: "i1".into(), name: "image".into(), media_type: MediaType::Png, size_bytes });
    }
    shown
}

#[test]
fn a_call_s_images_are_fetched_only_once_it_s_open() {
    let log = log([event(Seq::new(1), calls(&["a"])), event(Seq::new(2), with_an_image("a"))]);
    let mut m = Model::default();
    assert!(Around::read(&log, &m, Seq::new(1), &calls(&["a"])).images().is_empty(), "closed");

    m.folds.choose(ItemKey::Event(Seq::new(1), 0), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, Seq::new(1), &calls(&["a"])).images().len(), 1, "open");
}

#[test]
fn a_subagent_s_images_are_fetched_with_the_call_that_started_it() {
    let agent = subagents(&["a"]);
    let log = log([
        event(Seq::new(1), agent.clone()),
        event(Seq::new(2), within("a", calls(&["b"]))),
        event(Seq::new(3), within("a", with_an_image("b"))),
    ]);
    let mut m = Model::default();
    m.folds.choose(ItemKey::Event(Seq::new(2), 0), Fold::Output, true);
    assert!(Around::read(&log, &m, Seq::new(1), &agent).images().is_empty(), "the agent's call is closed");

    m.folds.choose(ItemKey::Event(Seq::new(1), 0), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, Seq::new(1), &agent).images().len(), 1, "opened, it gathers its subagent's");
}

/// A chat that reads by place holds only the agent's own events, so what a working subagent has done lately comes
/// from its record, images and all.
#[test]
fn a_working_subagent_s_latest_images_are_fetched_once_its_call_under_working_on_is_open() {
    let agent = subagents(&["agent"]);
    let mut log = SessionLog::by_place(Place::default());
    log.apply_event(SessionEvent { place: Place::new(1), ..event(Seq::new(1), agent.clone()) });
    let image = Attachment { id: "i1".into(), name: "image".into(), media_type: MediaType::Png, size_bytes: 1 };
    let read = Call::Read(Read { path: "a.png".into(), lines: None, pages: None });
    let output = CallOutput::Words { text: String::new() };
    let came_back = LatelyResult { output, is_error: false, cut: false, images: vec![image], denied: None };
    let made = LatelyCall { before: None, id: "read".into(), call: read, result: Some(came_back) };
    let mut m = Model::default();
    m.subagents.insert(Arc::new(working("agent", Lately { calls: vec![made], since: None })));
    m.folds.choose(ItemKey::Event(Seq::new(1), 0), Fold::Output, true);
    assert!(Around::read(&log, &m, Seq::new(1), &agent).images().is_empty(), "its read is closed");

    m.folds.choose(ItemKey::Call("read".into()), Fold::Output, true);

    assert_eq!(Around::read(&log, &m, Seq::new(1), &agent).images().len(), 1, "open, under Working on");
}

fn working(call: &str, lately: Lately) -> Subagent {
    Subagent { calls: 1, lately: Some(lately), ..record(call, SubagentStatus::Working) }
}

#[test]
fn a_result_from_the_agent_is_its_call_s_result_too() {
    let sent = from_the_agent(result("a"));
    let log = log([event(Seq::new(1), advised(&["a"])), event(Seq::new(2), sent.clone())]);
    let m = Model::default();

    let call = Around::read(&log, &m, Seq::new(1), &advised(&["a"]));

    assert_eq!(call.blocks[0].reply.result().map(Answer::shown), Some("from a"));
    assert!(Around::read(&log, &m, Seq::new(2), &sent).blocks[0].with_call, "so it isn't drawn twice");
}

#[test]
fn a_call_the_log_says_was_dropped_has_no_result_unless_one_came_after_all() {
    let dropped = EventBody::ToolCallDropped { tool_use_id: "a".into() };
    let gone = log([event(Seq::new(1), calls(&["a", "b"])), event(Seq::new(2), dropped.clone())]);
    let came = log([event(Seq::new(1), calls(&["a"])), event(Seq::new(2), dropped), event(Seq::new(3), result("a"))]);
    let m = Model::default();

    let beside = Around::read(&gone, &m, Seq::new(1), &calls(&["a", "b"]));
    let late = Around::read(&came, &m, Seq::new(1), &calls(&["a"]));

    assert!(matches!(beside.blocks[0].reply, Reply::Dropped));
    assert!(matches!(beside.blocks[1].reply, Reply::Nothing), "the call made beside it is still waiting");
    assert_eq!(late.blocks[0].reply.result().map(Answer::shown), Some("from a"), "it stands");
}

#[test]
fn a_change_of_mode_knows_the_mode_it_changed_from() {
    let changed_to = |mode| EventBody::AgentMode { mode };
    let log = log([
        event(Seq::new(1), changed_to(protocol::fixtures::plan_mode())),
        event(Seq::new(2), changed_to(protocol::fixtures::full_access())),
    ]);

    let around = Around::read(&log, &Model::default(), Seq::new(2), &changed_to(protocol::fixtures::full_access()));

    assert_eq!(around.mode_before, Some(protocol::fixtures::plan_mode()));
}

#[test]
fn a_result_without_its_call_nearby_is_drawn_where_it_is() {
    let log = log([event(Seq::new(1), calls(&["a"])), event(Seq::new(2), result("elsewhere"))]);
    let m = Model::default();

    assert!(!Around::read(&log, &m, Seq::new(2), &result("elsewhere")).blocks[0].with_call);
    let call = Around::read(&log, &m, Seq::new(1), &calls(&["a"]));
    assert!(
        matches!(call.blocks[0].reply, Reply::Nothing),
        "a call whose result hasn't come back has nothing under it"
    );
}

/// A failed command says how it exited beside what it printed.
#[test]
fn a_failed_result_says_how_its_command_exited() {
    let output = protocol::fixtures::command_output("no rule to make it", Some(2), Ended::Exited);
    let block = protocol::fixtures::typed_result("c", output, true);

    let reply = Reply::of(&block, &Model::default());
    let failure = reply.result().and_then(Answer::failure).unwrap();
    assert_eq!((failure.label().as_str(), failure.output), ("Exit 2", "no rule to make it"));
}

/// Retry asks the agent to carry on from the turn it stopped short on, so only the card that turn ended on has it, and
/// only while the agent is still stopped there.
#[test]
fn only_the_card_the_newest_turn_ended_on_offers_retry_until_something_is_sent() {
    let taken = |prompt| EventBody::PromptTaken { prompt };
    let turns = [
        event(Seq::new(1), prompt("Fix it")),
        event(Seq::new(2), taken(Seq::new(1))),
        event(Seq::new(3), failed_request(None)),
        event(Seq::new(4), turn_ended()),
        event(Seq::new(5), prompt("Continue")),
        event(Seq::new(6), taken(Seq::new(5))),
        event(Seq::new(7), failed_request(None)),
        event(Seq::new(8), turn_ended()),
    ];
    let twice = log(turns.clone());
    let mut at_work = AtWork { reply_seq: Seq::new(8), ..AtWork::default() };
    assert_eq!(retry_at(&twice, &at_work), None, "the record doesn't say the turn failed");

    at_work.failed = true;

    assert_eq!(retry_at(&twice, &at_work), Some(Seq::new(7)), "not the older turn's");
    let sent = log(turns.into_iter().chain([event(Seq::new(9), prompt("Try Sonnet"))]));
    assert_eq!(retry_at(&sent, &at_work), None, "something has been sent since");
}
