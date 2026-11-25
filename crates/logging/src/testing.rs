//! What a test's thread logs, captured.

use std::cell::RefCell;
use std::io::Write;
use std::sync::{Arc, Once};

use parking_lot::Mutex;
use serde_json::Value;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::{Layer as _, filter};

use crate::json::JsonLines;

thread_local! {
    /// Where what this thread logs goes, while a test on it captures it.
    static CAPTURING: RefCell<Option<Written>> = const { RefCell::new(None) };
}

/// Everything logged on this thread, at every level, from [`Logs::capture`] until it's dropped, as a process's log has
/// it. On the runtime of a `#[tokio::test]`, which runs every task on the test's thread, that's what its tasks log
/// too, but not what runs on a blocking thread.
pub struct Logs {
    written: Written,
}

impl Logs {
    /// Through one subscriber for the whole process, which writes what each thread logs where a test on it captures
    /// it, rather than a subscriber for each test, since with one, tracing asks only the thread that first logs at a
    /// place whether anything listens there, as tracing-core 0.1.36 does, and a test on another thread, where nothing
    /// does, would leave that place unheard for this one.
    ///
    /// # Panics
    ///
    /// If this thread is capturing already, or the process has a log.
    pub fn capture() -> Self {
        static SUBSCRIBED: Once = Once::new();
        SUBSCRIBED.call_once(|| {
            // Asked at each event, rather than once at each place, which is what makes it dynamic.
            let capturing = filter::dynamic_filter_fn(|_, _| CAPTURING.with(|to| to.borrow().is_some()));
            let lines = JsonLines::to(|| ThisThread).with_filter(capturing);
            let set = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(lines));
            assert!(set.is_ok(), "the process has a log already");
        });
        let written = Written::default();
        let before = CAPTURING.with(|to| to.borrow_mut().replace(written.clone()));
        assert!(before.is_none(), "this thread is capturing already");
        Self { written }
    }

    /// What's been logged so far, as it's written.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.written.0.lock()).into_owned()
    }

    /// Each event logged so far.
    ///
    /// # Panics
    ///
    /// If a line isn't JSON, which is what each is written as.
    pub fn events(&self) -> Vec<Value> {
        self.text()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line} isn't JSON: {e}")))
            .collect()
    }
}

impl Drop for Logs {
    fn drop(&mut self) {
        CAPTURING.with(|to| to.borrow_mut().take());
    }
}

#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

/// Writes to where a test on the thread writing captures what's logged.
struct ThisThread;

impl Write for ThisThread {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        CAPTURING.with(|to| {
            if let Some(written) = to.borrow().as_ref() {
                written.0.lock().extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
