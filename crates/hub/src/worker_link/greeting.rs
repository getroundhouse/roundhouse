//! Saying hello to a worker as its link opens, with the hub's git credential right behind it.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{Hello, HelloReply};

use super::WorkerLink;
use super::credential::Following;
use crate::versions;

impl WorkerLink {
    /// Says hello, and sets the git credential `following` follows, queued right behind the hello, before its answer,
    /// so nothing reaches the worker before the credential, not even what answers the worker's first ask, which can
    /// come with the hello's answer. The hello's answer has `within`, and the set's the limit
    /// [`Following::answered`] gives it after that. Returns `following` once both are answered, for the link to follow
    /// from then on.
    pub(super) async fn greet(&self, within: Duration, mut following: Following) -> Result<Following, ApiError> {
        let hello = Hello {
            protocol_version: versions::WORKERS.newest,
            oldest_protocol_version: Some(versions::WORKERS.oldest),
            // So a restart leaves it running, for the next hub. The hub says when it means it to stop.
            keep_running_without_hub: true,
        };
        let greeted = tokio::time::timeout(within, async {
            let hello = self.send(hello).await?;
            let set = following.queue().await?;
            Ok::<_, ApiError>((hello.await?, set))
        })
        .await;
        let Ok(greeted) = greeted else {
            return Err(ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {within:?}")));
        };
        let (HelloReply { protocol_version, build }, set) = greeted?;
        if !versions::WORKERS.speaks(protocol_version) {
            let message =
                format!("the worker chose protocol {protocol_version}, and this hub speaks {}", versions::WORKERS);
            return Err(ApiError::new(ErrorCode::VersionMismatch, message));
        }
        let _ = self.build.set(build);
        let _ = self.version.set(protocol_version);
        following.answered(set).await?;
        Ok(following)
    }
}
