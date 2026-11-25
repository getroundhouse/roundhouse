//! A log as a process starts it, in a process of its own, since a process has one.

use std::io::Write;
use std::sync::Arc;

use logging::{Levels, Log};
use parking_lot::Mutex;
use serde_json::Value;

#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_started_log_has_ours_at_info_the_rest_at_warn_what_the_log_crate_hears_and_a_panic_with_its_message() {
    let (lines, text) = (Written::default(), Written::default());
    let (to_lines, to_text) = (lines.clone(), text.clone());
    let log = Log::new(Levels::new(Some("loud=loudest")));
    log.json_to(move || to_lines.clone()).text_to(move || to_text.clone()).start().unwrap();

    tracing::info!(target: "hub::store", "ours");
    tracing::info!(target: "octocrab", "theirs");
    log::warn!(target: "gpui::window", "from the {} crate", "log");
    log::info!(target: "gpui::window", "theirs too");
    std::thread::Builder::new()
        .name("panicking".into())
        .spawn(|| panic!("said to nobody"))
        .unwrap()
        .join()
        .unwrap_err();

    let written = String::from_utf8(lines.0.lock().clone()).unwrap();
    let events: Vec<Value> = written.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let [unread, ours, logged, panicked] = events.as_slice() else { panic!("{written}") };
    assert_eq!((&unread["event"], &unread["fields"]["given"]), (&"log.levels.unread".into(), &"loud=loudest".into()));
    assert_eq!((&ours["target"], &ours["message"]), (&"hub::store".into(), &"ours".into()));
    assert_eq!((&logged["target"], &logged["message"]), (&"gpui::window".into(), &"from the log crate".into()));
    assert_eq!(logged.get("event"), None);
    assert_eq!(logged.get("fields"), None, "nothing of where in the source it's from: {logged}");
    assert_eq!((&panicked["event"], &panicked["level"]), (&"process.panicked".into(), &"ERROR".into()));
    assert_eq!(panicked["fields"]["thread"], "panicking");
    assert!(panicked["fields"]["at"].as_str().unwrap().contains("started.rs:"), "{panicked}");
    assert_eq!(panicked["fields"]["panic"], "said to nobody");
    let text = String::from_utf8(text.0.lock().clone()).unwrap();
    assert!(text.contains("INFO hub::store: ours"), "{text}");
    assert!(Log::new(Levels::new(None)).start().is_err(), "a process has one log");
}
