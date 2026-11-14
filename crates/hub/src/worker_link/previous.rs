//! Speaking protocol 3, the version before this one, to a worker of an older build, which the hub replaces with one of
//! its own once its workspace is idle. It goes with [`protocol::worker::previous`], at the next break, as "Changing the
//! protocol" in `docs/architecture.md` says, and so does each place that asks [`WorkerLink::speaks_previous`].

use protocol::client::ApiError;
use protocol::worker::{Reply, TitleFor, TitleReply, previous};

use super::WorkerLink;

impl WorkerLink {
    /// Whether its worker chose protocol 3 in its hello.
    pub(crate) fn speaks_previous(&self) -> bool {
        self.version.get() == Some(&previous::VERSION)
    }

    /// The worker's reply to a request of protocol 3, as `T`, as [`WorkerLink::call`] reads it.
    pub(crate) async fn call_previous<T: Reply>(&self, request: previous::Request) -> Result<T, ApiError> {
        self.send_call(|id| previous::Call { id, request }).await?.await
    }

    /// A title for a prompt, from the worker. Protocol 3 asks for one under a login with a request of its own, which a
    /// worker from before logins refuses rather than asking under its machine's own.
    pub(crate) async fn title(&self, title: TitleFor) -> Result<TitleReply, ApiError> {
        if self.speaks_previous() && title.login.is_some() {
            return self.call_previous(previous::Request::TitleUnderLogin(title)).await;
        }
        self.call(title).await
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use protocol::model::{AgentKind, AgentLogin, LoginToken};
    use protocol::worker::{TitleFor, TitleReply, WorkerResponse};
    use std::sync::Arc;

    use super::WorkerLink;

    #[tokio::test]
    async fn a_title_under_a_login_goes_to_a_worker_of_protocol_3_as_a_request_of_its_own() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = asked.clone();
        let link = WorkerLink::answering_previous(move |request| {
            kept.lock().push(request["method"].as_str().unwrap().to_string());
            Some(Ok(WorkerResponse::Title(TitleReply { title: "T".into() })))
        });
        let title = |login: Option<AgentLogin>| TitleFor {
            agent: AgentKind::Claude,
            prompt: "x".into(),
            images: vec![],
            login,
        };

        link.title(title(None)).await.unwrap();
        link.title(title(Some(AgentLogin { token: LoginToken::new("synthetic") }))).await.unwrap();

        assert_eq!(*asked.lock(), ["title_for", "title_under_login"]);
    }
}
