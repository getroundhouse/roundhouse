//! A session's record from a hub that keeps less of it, and what it says its agent is waiting on, and which way a prompt
//! was settled. A hub keeps the approvals in its database in the form it sends.

use protocol::client::HubNotice;
use protocol::event::TurnOutcome;
use protocol::frame;
use protocol::model::{AccountSource, MaxSetBy, OpenApproval, PromptLine, Seq, Session, SessionStatus, Settlement};

fn session(more: &str) -> Session {
    let notice = format!(
        r#"{{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{{"unasked":"everything","beyond":"asks"}},"effort":"high","status":"idle","last_seq":4,"created_at_ms":1,"updated_at_ms":2{more}}}"#
    );
    let notice: HubNotice = frame::decode(notice.as_bytes()).unwrap();
    let HubNotice::Session(session) = notice else { panic!("{notice:?}") };
    session
}

/// An older hub says none of it, which a client has to tell from a hub that says nothing waits.
#[test]
fn what_a_sessions_agent_is_waiting_on_keeps_its_form() {
    let says = session(r#","open_approvals":[{"seq":3,"approval_id":"a"}],"untaken":[4],"agent_opened":true"#);
    assert_eq!(says.open_approvals, Some(vec![OpenApproval { seq: Seq::new(3), approval_id: "a".into() }]));
    assert_eq!((says.untaken, says.agent_opened), (Some(vec![Seq::new(4)]), Some(true)));
    let nothing_waits = session(r#","open_approvals":[],"untaken":[],"agent_opened":false"#);
    assert_eq!((nothing_waits.open_approvals, nothing_waits.untaken), (Some(vec![]), Some(vec![])));
    assert_eq!(nothing_waits.agent_opened, Some(false));

    let older = session("");
    assert!(!older.compaction_waits(), "an older hub is asked, and has the last word");
    assert_eq!((older.open_approvals, older.untaken, older.agent_opened), (None, None, None));
    assert_eq!(
        serde_json::to_value(OpenApproval { seq: Seq::new(3), approval_id: "a".into() }).unwrap(),
        serde_json::json!({ "seq": 3, "approval_id": "a" })
    );
}

/// A hub from before sources says nothing of where a chat's login comes from, which reads as on none yet, and a source
/// from a newer peer reads as one this build doesn't know. Only a chat on the hub's is on an account.
#[test]
fn a_chats_account_source_keeps_its_form_and_a_hub_from_before_says_none() {
    let on_the_hubs = session(r#","account":"claude-1","account_source":"hub""#);
    let ambient = session(r#","account":"claude-1","account_source":"ambient""#);
    let newer = session(r#","account":"claude-1","account_source":"elsewhere""#);
    let older = session(r#","account":"claude-1""#);

    assert_eq!(
        (on_the_hubs.account_source, on_the_hubs.hub_account()),
        (Some(AccountSource::Hub), Some(&"claude-1".into()))
    );
    assert_eq!((ambient.account_source, ambient.hub_account()), (Some(AccountSource::Ambient), None));
    assert_eq!((newer.account_source, newer.hub_account()), (Some(AccountSource::Unknown), None));
    assert_eq!((older.account_source, older.hub_account()), (None, None));
    assert!(serde_json::to_string(&on_the_hubs).unwrap().contains(r#""account_source":"hub""#));
    assert!(serde_json::to_string(&ambient).unwrap().contains(r#""account_source":"ambient""#));
    assert!(!serde_json::to_string(&older).unwrap().contains("account_source"));
}

#[test]
fn which_way_a_prompt_was_settled_keeps_its_form() {
    let how = |settled: &str| {
        let line = format!(r#"{{"seq":7,"text":"Fix it","files":0,"settled":{{"seq":9,"place":6{settled}}}}}"#);
        serde_json::from_str::<PromptLine>(&line).unwrap().settled().unwrap().how
    };

    assert_eq!(how(r#","how":"taken""#), Some(Settlement::Taken));
    assert_eq!(how(r#","how":"dropped""#), Some(Settlement::Dropped));
    assert_eq!(how(r#","how":"later""#), Some(Settlement::Unknown));
    assert_eq!(how(""), None, "from an older hub");
    assert_eq!(serde_json::to_string(&Settlement::Dropped).unwrap(), r#""dropped""#);
}

#[test]
fn a_session_from_a_hub_that_doesnt_keep_replies_reads_as_read() {
    let line = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"xhigh","status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Session(session) = notice else { panic!("{notice:?}") };
    assert_eq!((session.reply_seq, session.read_seq), (Seq::default(), Seq::default()));
    assert!(!session.unread());
}

#[test]
fn a_running_session_from_a_hub_that_doesnt_keep_turn_starts_has_none() {
    let line = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"xhigh","status":"running","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Session(session) = notice else { panic!("{notice:?}") };
    assert_eq!(session.turn_started_at_ms, None);
}

#[test]
fn a_session_from_a_hub_that_doesnt_keep_how_full_the_context_is_has_none_and_its_form_stays_readable() {
    let older = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"xhigh","status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;
    let HubNotice::Session(session) = frame::decode::<HubNotice>(older).unwrap() else { panic!() };
    assert_eq!(session.context, None);

    // Spelled out, since a hub's stored rows hold this form too.
    let said = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"xhigh","status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2,"context":{"used_tokens":5,"max_tokens":10,"model_tokens":20,"max_set_by":"settings","parts":[{"name":"Messages","tokens":5}]}}"#;
    let HubNotice::Session(session) = frame::decode::<HubNotice>(said).unwrap() else { panic!() };
    let context = session.context.unwrap();
    assert_eq!((context.used_tokens, context.max_tokens, context.parts.len()), (5, 10, 1));
    assert_eq!((context.model_tokens, context.max_set_by), (Some(20), Some(MaxSetBy::Settings)));
}

/// A hub that doesn't keep how the newest turn ended says nothing of it, so no chat of its turns red for a turn that
/// failed. One that says a turn failed is stopped short until a turn starts, and an outcome a later hub names isn't a
/// failure.
#[test]
fn a_session_from_a_hub_that_doesnt_keep_how_its_turn_ended_didnt_fail() {
    let older = session("");
    let failed = session(r#","reply_seq":4,"reply_outcome":"error""#);
    let later = session(r#","reply_seq":4,"reply_outcome":"something_newer""#);

    assert_eq!(older.reply_outcome, None);
    assert!(!older.stopped_short());
    assert_eq!(failed.reply_outcome, Some(TurnOutcome::Error));
    assert!(failed.turn_failed() && failed.stopped_short());
    let running = Session { status: SessionStatus::Running, ..failed.clone() };
    assert!(!running.stopped_short(), "a turn has started again");
    assert!(!later.turn_failed());
    assert!(serde_json::to_string(&failed).unwrap().contains(r#""reply_outcome":"error""#));
    assert!(!serde_json::to_string(&older).unwrap().contains("reply_outcome"));
}

/// A chat's record always names its model, its approval mode and its effort, each a field of its own, since it always
/// runs with them.
#[test]
fn a_session_always_names_its_model_mode_and_effort() {
    let named = session("");
    assert_eq!((named.model.as_str(), named.effort.clone()), ("opus", protocol::capability::settings::Effort::High));
    assert_eq!(named.mode, protocol::fixtures::full_access());

    let line = r#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"high","status":"idle","last_seq":4,"created_at_ms":1,"updated_at_ms":2}"#;
    for (named, left) in [
        (r#""model":"opus","#, ""),
        (r#""mode":{"unasked":"everything","beyond":"asks"},"#, ""),
        (r#""effort":"high","#, ""),
        (r#""model":"opus""#, r#""model":null"#),
        (r#""model":"opus""#, r#""model":"default""#),
        (r#""mode":{"unasked":"everything","beyond":"asks"}"#, r#""mode":null"#),
        (r#""mode":{"unasked":"everything","beyond":"asks"}"#, r#""permission_mode":"bypassPermissions""#),
    ] {
        let line = line.replace(named, left);
        assert!(frame::decode::<HubNotice>(line.as_bytes()).is_err(), "{named} to {left:?}");
    }
}

/// A mode from a newer build, which its agent went into by itself, still reads, as one this build doesn't know.
#[test]
fn a_session_in_a_mode_this_build_doesnt_know_still_reads() {
    let newer = session("").mode;
    let line = r#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"what_it_learned","beyond":"asks"},"effort":"high","status":"idle","last_seq":4,"created_at_ms":1,"updated_at_ms":2}"#;
    let HubNotice::Session(session) = frame::decode(line.as_bytes()).unwrap() else { panic!("not a session") };

    assert!(!session.mode.is_known());
    assert_eq!(session.mode.beyond, newer.beyond);
}
