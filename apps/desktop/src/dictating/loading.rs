//! Loading the model, on the thread that hears what's dictated: as dictation is turned on, and again for the next
//! recording once it has been let go of.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dictation::{Engine, EngineError};

/// How the model is loaded from its files' folder: the real model, or a test's.
pub(super) type Load = Arc<dyn Fn(&Path) -> Result<Box<dyn Engine>, EngineError> + Send + Sync>;

/// Loads the model, and logs how long it took, which is under a second unless Core ML compiles it. A failure is left to
/// whatever asked for the model, which logs it.
pub(super) fn timed(load: &Load, dir: &Path) -> Result<Box<dyn Engine>, EngineError> {
    let started = Instant::now();
    let engine = load(dir)?;
    let took_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(name: "dictation.model_loaded", took_ms, "The model loaded");
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use dictation::fake::FakeEngine;
    use dictation::{Engine, EngineError};
    use logging::testing::Logs;

    use super::{Load, timed};

    #[test]
    fn a_model_that_loads_is_logged_with_how_long_it_took() {
        let logs = Logs::capture();

        let load: Load = Arc::new(|_| Ok(Box::new(FakeEngine::new(&[])) as Box<dyn Engine>));
        assert!(timed(&load, Path::new("")).is_ok());

        let events = logs.events();
        let [loaded] = events.as_slice() else { panic!("{events:#?}") };
        assert_eq!(loaded["event"], "dictation.model_loaded");
        assert!(loaded["fields"]["took_ms"].is_u64(), "{loaded}");
    }

    #[test]
    fn one_that_fails_to_load_is_left_to_what_asked_for_it() {
        let logs = Logs::capture();

        let load: Load = Arc::new(|_| Err(EngineError("its files are damaged".to_string())));
        assert!(timed(&load, Path::new("")).is_err());

        assert!(logs.events().is_empty(), "{:?}", logs.events());
    }
}
