//! Waits that block the thread they're called on, by the system's clock.

use std::fmt::Display;
use std::time::{Duration, Instant};

use crate::pace::Pace;

/// Returns once `done` holds, looking again every few milliseconds, and panics with `failure` once `limit` has passed
/// without it.
#[track_caller]
pub fn until(limit: Duration, failure: impl Display, mut done: impl FnMut() -> bool) {
    until_ok(limit, || if done() { Ok(()) } else { Err(&failure) });
}

/// What `found` gives once it gives `Ok`, looking again every few milliseconds, and panics with the last `Err` it gave
/// once `limit` has passed without it, so the failure can say what was found instead.
#[track_caller]
pub fn until_ok<T, E: Display>(limit: Duration, found: impl FnMut() -> Result<T, E>) -> T {
    match looking(limit, found) {
        Ok(found) => found,
        Err(failure) => panic!("{failure}"),
    }
}

/// Returns once `done` holds of `on`, looking again every few milliseconds, and once `limit` has passed without it,
/// panics with `timed out waiting for {what}: ` and what `said` makes of `on`, such as what a window last drew, so the
/// failure says what there was instead. `said` is called only then, so it can take as long as it likes. Both are given
/// `on` rather than taking it themselves, since they'd both borrow it.
#[track_caller]
pub fn until_or_say<S: ?Sized>(
    limit: Duration,
    what: impl Display,
    on: &mut S,
    mut done: impl FnMut(&mut S) -> bool,
    said: impl FnOnce(&mut S) -> String,
) {
    let held = looking(limit, || if done(on) { Ok(()) } else { Err(()) }).is_ok();
    assert!(held, "timed out waiting for {what}: {}", said(on));
}

/// Whether `done` holds within `limit`, looking again every few milliseconds, for a caller that goes on either way,
/// such as one that kills a process that hasn't exited by then.
pub fn up_to(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    looking(limit, || if done() { Ok(()) } else { Err(()) }).is_ok()
}

/// Gives what a test checks doesn't happen `bound` to happen in, before the test checks. A test uses it only when
/// nothing can be observed instead, such as a reply saying the work it would come from is over: it passes whenever
/// what it checks for comes later than `bound`, so it can miss a regression, and every run waits all of `bound`.
pub fn for_what_mustnt_happen(bound: Duration) {
    pause(bound);
}

/// What `found` gives once it's `Ok`, or the last `Err` it gave once `limit` has passed.
fn looking<T, E>(limit: Duration, mut found: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    let deadline = Instant::now() + limit;
    let mut pace = Pace::new();
    loop {
        match found() {
            Ok(found) => return Ok(found),
            Err(failure) if Instant::now() >= deadline => return Err(failure),
            Err(_) => pause(pace.next_pause()),
        }
    }
}

#[expect(clippy::disallowed_methods, reason = "where the waits that block a thread sleep, each for what its name says")]
fn pause(pause: Duration) {
    std::thread::sleep(pause);
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    use super::*;

    /// Longer than any of these tests waits for what holds.
    const LIMIT: Duration = Duration::from_secs(10);

    #[test]
    fn a_wait_returns_once_what_it_waits_for_holds() {
        let (done, set) = (Arc::new(AtomicBool::new(false)), Arc::new(Barrier::new(2)));
        let setting = std::thread::spawn({
            let (done, set) = (done.clone(), set.clone());
            move || {
                set.wait();
                done.store(true, Ordering::SeqCst);
            }
        });
        let mut looks = 0;

        until(LIMIT, "it never held", || {
            looks += 1;
            // Read before it's let go, so the look that lets it go never sees it set, however soon it's set.
            let held = done.load(Ordering::SeqCst);
            if looks == 2 {
                set.wait();
            }
            held
        });

        setting.join().unwrap();
        assert!(looks > 2, "it looked again after it was set, {looks} times in all");
    }

    #[test]
    #[should_panic(expected = "it never held")]
    fn a_wait_fails_with_what_it_was_given_once_its_limit_has_passed() {
        until(Duration::from_millis(20), "it never held", || false);
    }

    #[test]
    #[should_panic(expected = "only 3")]
    fn a_wait_for_what_s_found_fails_with_what_it_last_found() {
        let mut found = 0;

        until_ok(Duration::from_millis(20), || -> Result<(), String> {
            found = (found + 1).min(3);
            Err(format!("only {found}"))
        });
    }

    #[test]
    #[should_panic(expected = "timed out waiting for the door: it said 3 looks")]
    fn a_wait_that_says_what_there_was_says_it_only_once_its_limit_has_passed() {
        let mut looks = 0;

        until_or_say(
            Duration::from_millis(20),
            "the door",
            &mut looks,
            |looks| {
                *looks = (*looks + 1).min(3);
                false
            },
            |looks| format!("it said {looks} looks"),
        );
    }

    #[test]
    fn a_wait_up_to_a_limit_says_whether_what_it_waited_for_held() {
        let mut looks = 0;

        let held = up_to(LIMIT, || {
            looks += 1;
            looks == 3
        });
        let never = up_to(Duration::from_millis(20), || false);

        assert!(held);
        assert!(!never);
    }
}
