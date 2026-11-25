//! What this computer calls itself when pairing with another machine's hub.

use gpui_kit::{App, AppContext as _, Task};
use protocol::pairing::clean_device_name;

use crate::machine::Machine;

const FALLBACK: &str = "Roundhouse desktop";

/// Gives pictures and tests a fixed computer name, so no machine's own name enters a screenshot or fixture.
#[cfg(any(test, feature = "testing"))]
pub fn the_device_name_is(name: impl Into<String>, cx: &mut App) {
    let name = name.into();
    crate::machine::given(cx).name = std::sync::Arc::new(move || name.clone());
}

/// This computer's name, read now, as it's shown. On a Mac `scutil` runs a child process, so it's called off the main
/// thread.
pub(crate) fn read_now() -> String {
    clean_device_name(&super::system::computer_name(), FALLBACK)
}

/// Reads this computer's name off the main thread, since on a Mac `scutil` runs a child process.
pub(crate) fn read_in_background(cx: &App) -> Task<String> {
    let name = Machine::of(cx).name;
    cx.background_spawn(async move { clean_device_name(&name(), FALLBACK) })
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use protocol::pairing::{MAX_DEVICE_NAME_CHARS, device_name_from};

    use super::*;

    #[test]
    fn clean_trims_strips_controls_and_falls_back_when_empty() {
        assert_eq!(clean_device_name("  Ada’s MacBook Air \n", FALLBACK), "Ada’s MacBook Air");
        assert_eq!(clean_device_name("Ada\u{202e}riA\u{feff}", FALLBACK), "AdariA");
        assert_eq!(clean_device_name("   \n\t ", FALLBACK), FALLBACK);
        assert_eq!(clean_device_name(&"a".repeat(140), FALLBACK).chars().count(), MAX_DEVICE_NAME_CHARS);
    }

    #[test]
    fn this_machines_own_name_cleans_to_one_a_hub_accepts() {
        let cleaned = clean_device_name(&super::super::system::computer_name(), FALLBACK);
        assert!(device_name_from(&cleaned).is_ok(), "{cleaned:?}");
    }

    #[gpui_kit::test]
    fn background_lookup_resolves_after_spawning(cx: &mut TestAppContext) {
        cx.update(|cx| the_device_name_is("  Ada’s MacBook Air\n", cx));
        let (tx, rx) = flume::bounded(1);
        cx.update(|cx| {
            let task = read_in_background(cx);
            cx.spawn(async move |_| {
                let _ = tx.send(task.await);
            })
            .detach();
        });
        assert!(rx.try_recv().is_err(), "resolved on the background executor, not synchronously");
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), "Ada’s MacBook Air");
    }
}
