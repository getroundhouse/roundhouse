//! What a worker was last told of one thing, and the tellings that change it, which go one at a time, with at most one
//! waiting for its turn.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// What a worker was last told of one thing, such as whether a client watches one of its terminals, under a lock each
/// telling holds for its turn, so tellings go one at a time, and what's told last is what's so once it stops changing.
/// A telling reads what's so once it has its turn, so one asked for while another waits for its turn would tell what
/// that one tells, and is left to it. A worker that's slow to answer then keeps one telling waiting behind the one it
/// holds up, rather than one for each change meanwhile.
#[derive(Default)]
pub(super) struct Tellings<T> {
    told: Arc<AsyncMutex<T>>,
    waiting: AtomicBool,
}

impl<T> Tellings<T> {
    pub(super) fn new(told: T) -> Self {
        Self { told: Arc::new(AsyncMutex::new(told)), waiting: AtomicBool::new(false) }
    }

    /// Takes in that a telling is asked for, and says whether it's to wait for its turn, which it is unless one is
    /// waiting already. One that is to wait counts as waiting until it has its turn.
    pub(super) fn asked(&self) -> bool {
        !self.waiting.swap(true, Ordering::AcqRel)
    }

    /// A telling's turn, once the one under way is done, with what the worker was last told. A telling asked for once
    /// this has its turn waits for another.
    pub(super) async fn turn(&self) -> OwnedMutexGuard<T> {
        let told = self.told.clone().lock_owned().await;
        self.waiting.store(false, Ordering::Release);
        told
    }
}

#[cfg(test)]
mod tests {
    use super::Tellings;

    /// A telling asked for while another waits for its turn is left to that one, and one asked for once that one has
    /// its turn waits for its own.
    #[tokio::test]
    async fn a_telling_asked_for_while_another_waits_is_left_to_it() {
        let tellings = Tellings::new(());
        let under_way = tellings.turn().await;

        let asked = [tellings.asked(), tellings.asked(), tellings.asked()];
        drop(under_way);
        let _turn = tellings.turn().await;

        assert_eq!(asked, [true, false, false], "only the first waits");
        assert!(tellings.asked(), "asked for once the one waiting had its turn");
        assert!(!tellings.asked());
    }
}
