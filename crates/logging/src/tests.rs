use serde_json::{Value, json};
use tracing::{info, info_span, warn};

use crate::kept::moved_aside;
use crate::levels::Levels;
use crate::move_aside_if_long;
use crate::testing::Logs;

/// `event` without what differs from run to run, which is checked to be there.
fn without_its_time(mut event: Value) -> Value {
    let time = event.as_object_mut().unwrap().remove("time").unwrap();
    assert!(time.as_str().unwrap().ends_with('Z'), "{time}");
    event
}

#[test]
fn an_event_is_a_line_with_its_name_its_message_its_fields_and_what_its_in() {
    let logs = Logs::capture();

    let outer = info_span!("workspace", workspace_id = "w1", session_id = "outer").entered();
    let inner = info_span!("session", session_id = "s1", turn = tracing::field::Empty).entered();
    inner.record("turn", 3);
    warn!(
        name: "github.read.failed",
        tries = 2,
        ms = 2_u128,
        again = true,
        error = %"line one\nline two",
        "Couldn't read"
    );
    drop((inner, outer));
    info!("Unnamed, in nothing");

    let events: Vec<Value> = logs.events().into_iter().map(without_its_time).collect();
    assert_eq!(
        events,
        [
            json!({
                "level": "WARN",
                "target": "logging::tests",
                "event": "github.read.failed",
                "message": "Couldn't read",
                "fields": { "tries": 2, "ms": 2, "again": true, "error": "line one\nline two" },
                "context": { "workspace_id": "w1", "session_id": "s1", "turn": 3 },
            }),
            json!({ "level": "INFO", "target": "logging::tests", "message": "Unnamed, in nothing" }),
        ]
    );
    assert_eq!(logs.text().lines().count(), 2, "an event is one line, whatever is in it");
}

#[test]
fn a_tests_logs_are_what_its_thread_logs_while_it_captures_them() {
    // One place, since tracing can settle whether anything listens there as the first thread reaches it.
    fn log(from: &str) {
        info!(from, "Logged");
    }
    let logs = Logs::capture();

    std::thread::spawn(|| log("elsewhere")).join().unwrap();
    log("here");

    assert_eq!(logs.events().len(), 1, "{}", logs.text());
    assert_eq!(logs.events()[0]["fields"]["from"], "here");
    drop(logs);
    log("after");
    assert_eq!(Logs::capture().text(), "");
}

#[test]
fn ours_are_logged_at_info_and_the_rest_at_warn_unless_levels_are_given() {
    let usual = Levels::new(None).filter().to_string();
    for directive in ["warn", "hub=info", "desktop=info"] {
        assert!(usual.split(',').any(|given| given == directive), "{directive} isn't in {usual}");
    }
    assert_eq!(Levels::new(Some(" ")).filter().to_string(), usual, "nothing given");
    assert_eq!(Levels::new(Some("error,hub=debug")).filter().to_string(), "hub=debug,error");

    let unread = Levels::new(Some("hub=loud"));

    assert_eq!(unread.unread(), Some("hub=loud"));
    assert_eq!(unread.filter().to_string(), usual);
}

#[test]
fn a_long_log_is_moved_aside_in_place_of_the_one_before_and_a_short_one_is_left() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("hub.log");
    let before = moved_aside(&log);
    move_aside_if_long(&log);
    assert!(!before.exists(), "there's no log");

    std::fs::write(&log, "short").unwrap();
    move_aside_if_long(&log);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "short");

    std::fs::write(&before, "older").unwrap();
    std::fs::File::options().write(true).open(&log).unwrap().set_len(10 * 1024 * 1024 + 1).unwrap();
    move_aside_if_long(&log);

    assert!(!log.exists());
    assert_eq!(std::fs::metadata(&before).unwrap().len(), 10 * 1024 * 1024 + 1);
}
