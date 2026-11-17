//! Waits in a tokio task, by tokio's clock. On a test's paused clock, a wait moves the clock on as it waits, as a sleep
//! does, and its limit is a time on that clock. Since that clock runs ahead of the system's, a wait on it looks again
//! less often the further ahead it runs, up to every 100 ms, so a wait of minutes on it doesn't take seconds.

use std::fmt::Display;
use std::future::{Future, ready};
use std::time::Duration;

use tokio::time::Instant;

use crate::pace::Pace;

/// Returns once `done` holds, looking again every few milliseconds, and panics with `failure` once `limit` has passed
/// without it.
pub async fn until(limit: Duration, failure: impl Display, mut done: impl FnMut() -> bool) {
    until_ok(limit, || if done() { Ok(()) } else { Err(&failure) }).await;
}

/// What `found` gives once it gives `Ok`, looking again every few milliseconds, and panics with the last `Err` it gave
/// once `limit` has passed without it, so the failure can say what was found instead.
pub async fn until_ok<T, E: Display>(limit: Duration, mut found: impl FnMut() -> Result<T, E>) -> T {
    until_ok_async(limit, || ready(found())).await
}

/// As [`until_ok`], for what has to be awaited to be found, such as whether something answers on a port. `found` is
/// called again for each look, so the future it gives can't borrow what the closure holds, and moves in what it needs
/// instead, such as a reference: `|| async move { connect(socket).await }`.
pub async fn until_ok_async<T, E: Display, F>(limit: Duration, found: impl FnMut() -> F) -> T
where
    F: Future<Output = Result<T, E>>,
{
    match looking(limit, found).await {
        Ok(found) => found,
        Err(failure) => panic!("{failure}"),
    }
}

/// Whether `done` holds within `limit`, looking again every few milliseconds, for a caller that goes on either way,
/// such as one that says what didn't happen in an error of its own.
pub async fn up_to(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    looking(limit, || ready(if done() { Ok(()) } else { Err(()) })).await.is_ok()
}

/// Gives what a test checks doesn't happen `bound` to happen in, before the test checks. A test uses it only when
/// nothing can be observed instead, such as a reply saying the work it would come from is over: it passes whenever
/// what it checks for comes later than `bound`, so it can miss a regression, and every run waits all of `bound`.
pub async fn for_what_mustnt_happen(bound: Duration) {
    pause(bound).await;
}

/// What `found` gives once it's `Ok`, or the last `Err` it gave once `limit` has passed.
async fn looking<T, E, F>(limit: Duration, mut found: impl FnMut() -> F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let (started, started_by_the_system) = (Instant::now(), std::time::Instant::now());
    let mut pace = Pace::new();
    loop {
        match found().await {
            Ok(found) => return Ok(found),
            Err(failure) if started.elapsed() >= limit => return Err(failure),
            Err(_) => {
                let ahead = started.elapsed().saturating_sub(started_by_the_system.elapsed());
                pause(pace.next_pause_ahead(ahead)).await;
            }
        }
    }
}

#[expect(clippy::disallowed_methods, reason = "where the waits in a task sleep, each for what its name says")]
async fn pause(pause: Duration) {
    tokio::time::sleep(pause).await;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[tokio::test]
    async fn a_wait_returns_once_what_it_waits_for_holds() {
        let done = Arc::new(AtomicBool::new(false));
        let mut looks = 0;

        until(Duration::from_secs(10), "it never held", || {
            looks += 1;
            if looks == 2 {
                let done = done.clone();
                tokio::spawn(async move { done.store(true, Ordering::SeqCst) });
            }
            done.load(Ordering::SeqCst)
        })
        .await;

        assert!(looks > 2, "it looked again after it was set, {looks} times in all");
    }

    /// A minute on the paused clock, which takes no time but its looks.
    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "only 3")]
    async fn a_wait_fails_with_what_it_last_found_once_its_limit_on_tokio_s_clock_has_passed() {
        let mut found = 0;

        until_ok_async(Duration::from_mins(1), || {
            found = (found + 1).min(3);
            let found = format!("only {found}");
            async move { Err::<(), _>(found) }
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_up_to_a_limit_says_whether_what_it_waited_for_held() {
        let started = Instant::now();

        let never = up_to(Duration::from_secs(60), || false).await;

        assert!(!never);
        assert!(started.elapsed() >= Duration::from_secs(60), "{:?}", started.elapsed());
    }
}
