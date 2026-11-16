//! What the hub logs, captured, for a test of what goes in its log.

use std::cell::RefCell;
use std::io::Write;
use std::sync::{Arc, Once};

use parking_lot::Mutex;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

thread_local! {
    /// Where what this thread logs goes, while a test on it captures it.
    static CAPTURING: RefCell<Option<Written>> = const { RefCell::new(None) };
}

/// Everything logged on this thread, at every level, from [`Logs::capture`] until it's dropped. On the runtime of a
/// `#[tokio::test]`, which runs every task on the test's thread, that's what its tasks log too, but not what runs on
/// a blocking thread.
pub(crate) struct Logs {
    written: Written,
}

impl Logs {
    /// Through one subscriber for the whole process, which writes what each thread logs where a test on it captures
    /// it, rather than a subscriber for each test, since with one, tracing asks only the thread that first logs at a
    /// place whether anything listens there, as tracing-core 0.1.36 does, and a test on another thread, where nothing
    /// does, would leave that place unheard for this one.
    pub(crate) fn capture() -> Self {
        static SUBSCRIBED: Once = Once::new();
        SUBSCRIBED.call_once(|| {
            // Asked at each event, rather than once at each place, which is what makes it dynamic.
            let capturing =
                tracing_subscriber::filter::dynamic_filter_fn(|_, _| CAPTURING.with(|to| to.borrow().is_some()));
            let layer = tracing_subscriber::fmt::layer().with_writer(|| ThisThread).with_ansi(false);
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer.with_filter(capturing)))
                .unwrap();
        });
        let written = Written::default();
        CAPTURING.with(|to| *to.borrow_mut() = Some(written.clone()));
        Self { written }
    }

    /// What's been logged so far.
    pub(crate) fn text(&self) -> String {
        String::from_utf8(self.written.0.lock().clone()).unwrap()
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
