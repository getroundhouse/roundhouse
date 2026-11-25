//! Starting a process's log.

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::json::JsonLines;
use crate::levels::Levels;
use crate::panics;

/// A process's log, and where it goes. Only a process's `main` starts one, since a test has a log of its own.
pub struct Log {
    levels: Levels,
    json: Option<BoxMakeWriter>,
    text: Option<BoxMakeWriter>,
}

/// The process has a log already.
#[derive(Debug, thiserror::Error)]
#[error("this process has started its log already")]
pub struct AlreadyStarted;

impl Log {
    /// One that goes nowhere yet.
    pub fn new(levels: Levels) -> Self {
        Self { levels, json: None, text: None }
    }

    /// Writes a line of JSON for each event, as a log's file has it.
    #[must_use]
    pub fn json_to(mut self, to: impl for<'w> MakeWriter<'w> + Send + Sync + 'static) -> Self {
        self.json = Some(BoxMakeWriter::new(to));
        self
    }

    /// Writes each event as text, for someone reading it as it happens in a terminal.
    #[must_use]
    pub fn text_to(mut self, to: impl for<'w> MakeWriter<'w> + Send + Sync + 'static) -> Self {
        self.text = Some(BoxMakeWriter::new(to));
        self
    }

    /// Logs everything the process does from now on, and each panic. What it's built on that logs with the `log`
    /// crate, such as GPUI, is logged too.
    pub fn start(self) -> Result<(), AlreadyStarted> {
        let Self { levels, json, text } = self;
        let unread = levels.unread().map(str::to_owned);
        tracing_subscriber::registry()
            .with(levels.filter())
            .with(json.map(JsonLines::to))
            .with(
                text.map(|to| {
                    tracing_subscriber::fmt::layer().with_writer(to).with_ansi(false).log_internal_errors(false)
                }),
            )
            .try_init()
            .map_err(|_| AlreadyStarted)?;
        panics::log_each();
        if let Some(given) = unread {
            tracing::warn!(name: "log.levels.unread", given, "Couldn't read the levels to log at, so logging at the usual ones");
        }
        Ok(())
    }
}
