use std::sync::Arc;

use tokio::sync::watch;

/// What holds something back, such as a stand-in's answer, until a test opens it.
#[derive(Clone, Debug)]
pub struct Gate(Arc<watch::Sender<bool>>);

impl Gate {
    /// One nothing has passed yet.
    pub fn closed() -> Self {
        Gate(Arc::new(watch::Sender::new(false)))
    }

    /// Lets everything it holds back go, and everything after.
    pub fn open(&self) {
        self.0.send_replace(true);
    }

    /// Once it's open.
    pub async fn opened(&self) {
        let mut opens = self.0.subscribe();
        // It holds its own sender, so it's never told that has gone.
        let _open = opens.wait_for(|open| *open).await;
    }
}
