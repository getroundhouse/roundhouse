use super::Progress;
use tracing::Instrument as _;

/// Runs `work` to its end on a task of its own, and waits for it, so stopping the job this is in doesn't cut it off. For
/// what mustn't stop partway once it has begun, such as recording what a worker answered and saying so. A job stopped
/// meanwhile runs its after-step once this has ended, and tells it this began. Nothing limits it, so it waits on
/// nothing outside the hub. What it says with [`super::step`] isn't kept.
pub(crate) async fn uncut<F>(work: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let held = Progress::uncut_begins();
    let running = tokio::spawn(
        async move {
            let output = work.await;
            drop(held);
            output
        }
        .in_current_span(),
    );
    match running.await {
        Ok(output) => output,
        // On to whatever waits, as if `work` had run here.
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        // Only as the runtime shuts down, which ends whatever waits too.
        Err(_) => std::future::pending().await,
    }
}
