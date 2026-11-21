use protocol::capability::{Capabilities, Control, conversation::Start};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::AgentKind;
use protocol::worker::{AgentControl, AgentSpec, WorkerRequest};
use serde_json::{Value, json};

#[test]
fn a_launch_says_what_it_starts_from_in_its_own_form() {
    let wire = json!({"session_id":"s","run":4,"agent":"codex","cwd":"/w","start":"new",
        "settings":{"model":"gpt-6-luna"},"env":[]});
    let mut spec: AgentSpec = serde_json::from_value(wire).unwrap();

    assert_eq!(spec.agent, AgentKind::Codex);
    assert_eq!(spec.start, Start::New { conversation: None });
    assert_eq!(serde_json::from_value::<AgentSpec>(serde_json::to_value(&spec).unwrap()).unwrap(), spec);

    spec.start = Start::Resume { conversation: "chosen-by-cli".into() };
    let sent = serde_json::to_value(&spec).unwrap();
    assert_eq!((&sent["start"], &sent["conversation"]), (&json!("resume"), &json!("chosen-by-cli")));
    assert!(
        sent.get("agent_session_id").is_none(),
        "only the form from before `Start` has its ID and whether to resume it"
    );
    assert_eq!(serde_json::from_value::<AgentSpec>(sent).unwrap(), spec);
}

#[test]
fn legacy_initialization_has_no_claims_and_new_claims_survive_the_log_shape() {
    let old: EventBody = serde_json::from_value(json!({"type":"agent_init","agent_session_id":"a"})).unwrap();
    let EventBody::AgentInit { capabilities: None, .. } = old else { panic!("{old:?}") };
    let new: EventBody = serde_json::from_value(json!({"type":"agent_init","agent_session_id":"a",
        "capabilities":{"prompts":{"steer":{}}}}))
    .unwrap();
    let EventBody::AgentInit { capabilities: Some(ref claims), .. } = new else { panic!("{new:?}") };
    assert!(claims.prompts.steer.is_some());
    assert_eq!(serde_json::from_value::<EventBody>(serde_json::to_value(&new).unwrap()).unwrap(), new);
}

/// The mode an agent reports it starts in, and each it goes into, which the log keeps for good.
#[test]
fn the_mode_an_agent_reports_keeps_its_form() {
    let init = json!({"type":"agent_init","agent_session_id":"a","model":"opus","cwd":"/w",
        "mode":{"unasked":"reads","beyond":"asks","plan":true},"agent_version":"2.1"});
    let read: EventBody = serde_json::from_value(init.clone()).unwrap();
    let EventBody::AgentInit { mode: Some(mode), .. } = read else { panic!("{read:?}") };
    assert_eq!(mode, protocol::fixtures::plan_mode());
    assert_eq!(serde_json::to_value(&read).unwrap(), init);

    let changed = json!({"type":"agent_mode","mode":{"unasked":"everything","beyond":"asks"}});
    let read: EventBody = serde_json::from_value(changed.clone()).unwrap();
    assert_eq!(read, EventBody::AgentMode { mode: protocol::fixtures::full_access() });
    assert_eq!(serde_json::to_value(&read).unwrap(), changed);
}

#[test]
fn a_control_has_a_new_envelope_and_keeps_its_capability_requirements() {
    let request = WorkerRequest::AgentControl(AgentControl { session_id: "s".into(), control: Control::Cancel });
    let sent = serde_json::to_value(&request).unwrap();

    assert_eq!(sent, json!({"method":"agent_control","session_id":"s","control":"cancel"}));
    assert_eq!(serde_json::from_value::<WorkerRequest>(sent).unwrap(), request);
    assert!(!Control::Cancel.allowed(&Capabilities::default()));
}

#[test]
fn current_calls_keep_typed_input_once_and_old_calls_dont_fail_their_event() {
    let call: ContentBlock = serde_json::from_value(json!({"type":"call","id":"c","call":
        {"kind":"write","path":"a.txt","content":"only once"}}))
    .unwrap();
    let sent = serde_json::to_string(&call).unwrap();

    assert_eq!(sent.matches("only once").count(), 1);
    assert_eq!(serde_json::from_str::<ContentBlock>(&sent).unwrap(), call);
    let old = json!({"type":"tool_use","id":"c","name":"Write","input":{"file_path":"a.txt","content":"old"}});
    let decoded: ContentBlock = serde_json::from_value(old).unwrap();
    assert_eq!(decoded, ContentBlock::Unknown, "the hub typed an old one, so a client never needs to read it");
    let unknown: ContentBlock = serde_json::from_value(json!({"type":"future_block","input":Value::Null})).unwrap();
    assert_eq!(unknown, ContentBlock::Unknown);
}

/// Each control a run of this build claims, as a hub sends it to a worker of this protocol, spelled out, since a
/// worker of this build has to read what any later hub sends it, and these types are what a hub sends it until the
/// protocol breaks.
#[test]
fn every_control_a_run_of_this_build_claims_keeps_its_form() {
    use protocol::capability::approvals::{AfterDenial, Decision, Remember};
    use protocol::capability::asking::PlanContext;
    use protocol::capability::asking::QuestionAnswer;
    use protocol::capability::prompts::Extras;
    use protocol::capability::settings::{Effort, Settings};
    use protocol::model::{ModelName, Seq};
    use protocol::prompt::Prompt;

    let decide = |decision| Control::Decide { approval: "a".into(), decision };
    let controls = [
        (
            Control::Prompt { seq: Seq::new(3), prompt: Prompt::new("go"), extras: Extras::default() },
            json!({"control":"prompt","seq":3,"prompt":{"text":"go"},"extras":{}}),
        ),
        (Control::Cancel, json!({"control":"cancel"})),
        (
            Control::Compact { asked: Seq::new(9), keeping: Some("The API".into()) },
            json!({"control":"compact","asked":9,"keeping":"The API"}),
        ),
        (
            Control::Change(Settings {
                model: Some(ModelName::named("opus")),
                effort: Some(Effort::High),
                ..Settings::default()
            }),
            json!({"control":"change","model":"opus","mode":null,"effort":"high","speed":null,"thinking":null,
                "persona":null,"style":null,"verbosity":null,"advisor":null,"budget":null}),
        ),
        (
            decide(Decision::Allow { remember: Remember::Once }),
            json!({"control":"decide","approval":"a","decision":{"decision":"allow","remember":{"remember":"once"}}}),
        ),
        (
            decide(Decision::Deny { words: Some("No".into()), then: AfterDenial::Stop, keep: None }),
            json!({"control":"decide","approval":"a","decision":{"decision":"deny","words":"No","then":"stop"}}),
        ),
        (
            decide(Decision::Answer { answers: vec![QuestionAnswer { picked: vec![1], typed: Some("Bo".into()) }] }),
            json!({"control":"decide","approval":"a","decision":{"decision":"answer","answers":[{"picked":[1],"typed":"Bo"}]}}),
        ),
        (
            decide(Decision::ApprovePlan { context: PlanContext::Kept }),
            json!({"control":"decide","approval":"a","decision":{"decision":"approve_plan","context":"kept"}}),
        ),
        (
            decide(Decision::KeepPlanning { words: Some("Smaller".into()) }),
            json!({"control":"decide","approval":"a","decision":{"decision":"keep_planning","words":"Smaller"}}),
        ),
    ];
    for (control, form) in controls {
        let mut sent = form.clone();
        sent.as_object_mut().unwrap().insert("method".into(), json!("agent_control"));
        sent.as_object_mut().unwrap().insert("session_id".into(), json!("s"));
        let request = WorkerRequest::AgentControl(AgentControl { session_id: "s".into(), control });
        assert_eq!(serde_json::to_value(&request).unwrap(), sent);
        assert_eq!(serde_json::from_value::<WorkerRequest>(sent).unwrap(), request, "{form}");
    }
}

#[test]
fn classifier_approval_mode_keeps_its_form() {
    use protocol::capability::approvals::{ApprovalMode, Beyond, Unasked};

    let classifier = ApprovalMode { unasked: Unasked::Reads, beyond: Beyond::Classifier, sandbox: None, plan: false };
    assert!(classifier.is_known());
    let wire = serde_json::to_value(classifier).unwrap();
    assert_eq!(wire, json!({"unasked":"reads","beyond":"classifier"}));
    assert_eq!(serde_json::from_value::<ApprovalMode>(wire).unwrap(), classifier);
}

/// A decision the log keeps can be of a kind, or have a part, from a later build, which this one reads as unknown
/// rather than failing the event it's in.
#[test]
fn a_decision_or_a_part_of_one_this_build_doesnt_know_reads_as_unknown() {
    use protocol::capability::approvals::{AfterDenial, Decision, Lasts, Remember};
    use protocol::capability::asking::ServerAnswer;

    let read = |form| serde_json::from_value::<Decision>(form).unwrap();
    assert_eq!(read(json!({"decision":"overrule","why":"Tested"})), Decision::Unknown);
    assert_eq!(
        read(json!({"decision":"allow","remember":{"remember":"for_the_project"}})),
        Decision::Allow { remember: Remember::Unknown }
    );
    assert_eq!(
        read(json!({"decision":"deny","then":"rewind"})),
        Decision::Deny { words: None, then: AfterDenial::Unknown, keep: None }
    );
    assert_eq!(
        read(json!({"decision":"grant","access":[],"lasts":"day"})),
        Decision::Grant { access: vec![], lasts: Lasts::Unknown }
    );
    assert_eq!(read(json!({"decision":"respond","answer":"defer"})), Decision::Respond(ServerAnswer::Unknown));
}
