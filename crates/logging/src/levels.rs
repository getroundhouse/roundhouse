//! Which levels a process logs at.

use tracing_subscriber::EnvFilter;

/// The crates whose events are ours, by the name each one's events are from, logged at `info`. What they're built on is
/// logged at `warn`.
pub const OURS: &[&str] = &["agents", "desktop", "github", "hub", "logging", "providers", "roundhoused", "worker"];

/// What's logged, and at which levels.
pub struct Levels {
    filter: EnvFilter,
    unread: Option<String>,
}

impl Levels {
    /// The levels `directives` gives, as `ROUNDHOUSE_LOG` has them, such as `info,hub=debug`, or the usual ones where
    /// it gives none, or can't be read, which the log then says.
    pub fn new(directives: Option<&str>) -> Self {
        let given = directives.filter(|given| !given.trim().is_empty());
        match given.map(|given| EnvFilter::try_new(given).map_err(|_| given)) {
            Some(Ok(filter)) => Self { filter, unread: None },
            Some(Err(given)) => Self { filter: EnvFilter::new(usual()), unread: Some(given.to_owned()) },
            None => Self { filter: EnvFilter::new(usual()), unread: None },
        }
    }

    pub(crate) fn filter(self) -> EnvFilter {
        self.filter
    }

    /// What was given that couldn't be read.
    pub(crate) fn unread(&self) -> Option<&str> {
        self.unread.as_deref()
    }
}

/// What everything is logged at unless a person says otherwise.
fn usual() -> String {
    let ours = OURS.iter().map(|target| format!("{target}=info"));
    std::iter::once("warn".to_owned()).chain(ours).collect::<Vec<_>>().join(",")
}
