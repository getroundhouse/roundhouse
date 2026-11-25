use std::time::Duration;

use tokio::time::MissedTickBehavior;

/// Has `check` check at once, and every `every` after, at the time `wall_clock` says then, each once the last has
/// finished, so no two run at once, until `stopped`, which a check under way runs to its end past. One that comes late,
/// as on a hub that's busy or after a check that took long, isn't followed by others to catch up.
pub(super) async fn check_by_the_wall_clock<F>(
    every: Duration,
    wall_clock: impl Fn() -> i64,
    stopped: impl Future<Output = ()>,
    mut check: impl FnMut(i64) -> F,
) where
    F: Future<Output = ()>,
{
    let mut checks = tokio::time::interval(every);
    checks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut stopped = std::pin::pin!(stopped);
    loop {
        tokio::select! {
            biased;
            () = &mut stopped => return,
            _ = checks.tick() => {}
        }
        check(wall_clock()).await;
    }
}
