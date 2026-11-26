//! Saying hello to a worker as its link opens, with what the hub has it use right behind it.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ProviderName, WorkspaceId};
use protocol::worker::{Hello, HelloReply};
use providers::Provider;
use tracing::{Instrument as _, Span};

use super::WorkerLink;
use super::following::{Credential, Following, Servers};
use crate::hub::Hub;
use crate::versions;

/// What a link follows, from its greeting on.
pub(super) struct Followings {
    pub(super) credential: Following<Credential>,
    pub(super) tool_servers: Following<Servers>,
}

impl Followings {
    /// What `link`, to a worker of `provider`, follows of `hub`.
    pub(super) fn of(link: &Arc<WorkerLink>, hub: &Hub, provider: &dyn Provider) -> Self {
        let left_to_machine = hub.leaves_github_to_machine(&ProviderName::from(provider.name()));
        let credential = Credential((!left_to_machine).then(|| hub.git_credential()));
        Self {
            credential: Following::new(Arc::clone(link), credential),
            tool_servers: Following::new(Arc::clone(link), Servers::new(hub.sent_tool_servers())),
        }
    }

    /// Follows each from now on, and has `hub` look at its agents again each time `link`, to `workspace_id`'s worker, has a
    /// tool server as it wasn't before.
    pub(super) fn follow(self, hub: &Arc<Hub>, link: &Arc<WorkerLink>, workspace_id: &WorkspaceId, span: &Span) {
        tokio::spawn(self.credential.follow().instrument(span.clone()));
        // Before the servers are followed, so it misses none.
        let reconnecting = Arc::clone(hub).reconnect_as_changed(workspace_id.clone(), Arc::clone(link));
        tokio::spawn(reconnecting.instrument(span.clone()));
        tokio::spawn(self.tool_servers.follow().instrument(span.clone()));
    }
}

impl WorkerLink {
    /// Says hello, and sets what `followings` follow, the git credential and then the tool servers, queued right behind
    /// the hello, before its answer, so nothing reaches the worker before them, not even what answers the worker's first
    /// ask, which can come with the hello's answer. So an agent the worker starts has both. The hello's answer has
    /// `within`, and each set's the limit [`Following::answered`] gives it after that. Returns `followings` once all are
    /// answered, for the link to follow from then on. A worker whose hello then fails has been sent the credential and
    /// the servers' keys too. *Unconfirmed:* queueing the sets before the hello's answer.
    pub(super) async fn greet(&self, within: Duration, mut followings: Followings) -> Result<Followings, ApiError> {
        let hello = Hello {
            protocol_version: versions::WORKERS.newest,
            oldest_protocol_version: Some(versions::WORKERS.oldest),
            // So a restart leaves it running, for the next hub. The hub says when it means it to stop.
            keep_running_without_hub: true,
        };
        let greeted = tokio::time::timeout(within, async {
            let hello = self.send(hello).await?;
            let credential = followings.credential.queue().await?;
            let tool_servers = followings.tool_servers.queue().await?;
            Ok::<_, ApiError>((hello.await?, credential, tool_servers))
        })
        .await;
        let Ok(greeted) = greeted else {
            return Err(ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {within:?}")));
        };
        let (HelloReply { protocol_version, build }, credential, tool_servers) = greeted?;
        if !versions::WORKERS.speaks(protocol_version) {
            let message =
                format!("the worker chose protocol {protocol_version}, and this hub speaks {}", versions::WORKERS);
            return Err(ApiError::new(ErrorCode::VersionMismatch, message));
        }
        let _ = self.build.set(build);
        let _ = self.version.set(protocol_version);
        let Followings { credential: following, tool_servers: servers } = &mut followings;
        tokio::try_join!(following.answered(credential), servers.answered(tool_servers))?;
        Ok(followings)
    }
}
