//! The workspace's pull request: reading it from the hub, and doing what its bar offers.

use gpui_kit::{AppContext as _, Context};
use protocol::model::PrState;

use super::{PaneEvent, WorkspacePane};
use crate::pull_request::{Action, Ask};

impl WorkspacePane {
    pub fn create_pull_request(&mut self, cx: &mut Context<Self>) {
        self.ask(Ask::CreatePr, cx);
    }

    /// Does what the pull request's bar offers.
    pub(crate) fn act(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Ask(ask) => self.ask(ask, cx),
            Action::Archive => cx.emit(PaneEvent::Archive(self.workspace().id.clone(), false)),
        }
    }

    /// Asks the agent in the chat showing, and shows it, so the agent can be seen taking it up.
    fn ask(&mut self, ask: Ask, cx: &mut Context<Self>) {
        let prompt = ask.prompt(self.base());
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, |t, cx| t.send_prompt(prompt, cx));
        }
        self.show_chat(cx);
    }

    /// The branch the pull request merges into, or the one the workspace started from while there's none to say.
    fn base(&self) -> &str {
        if let Some(base) = self.pull_request.pull_request().and_then(|pr| pr.base.as_ref()) {
            return base.as_str();
        }
        let from = self.workspace().base_ref.as_deref().unwrap_or("the default branch");
        from.strip_prefix("origin/").unwrap_or(from)
    }

    /// Asks the hub to read it now. The hub records what it finds on the workspace's record, which brings it here in
    /// order with everything else the hub says, so the answer is only taken from the reply while the record has none,
    /// such as from a hub too old to keep it there.
    pub(super) fn refresh_pull_request(&mut self, cx: &mut Context<Self>) {
        self.pull_request_asks += 1;
        let (client, id, asked) = (self.client.clone(), self.workspace().id.clone(), self.pull_request_asks);
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { client.workspace_pull_request(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                let unrecorded = t.workspace().pull_request.is_none();
                if unrecorded && t.pull_request.answer(asked == t.pull_request_asks, found) {
                    t.pass_on_pull_request(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// To each chat, for its line for a wait on the pull request.
    pub(super) fn pass_on_pull_request(&self, cx: &mut Context<Self>) {
        let standing = self.pull_request.for_a_wait();
        for (transcript, _) in self.transcripts.values() {
            transcript.update(cx, |t, cx| t.set_pull_request(standing.clone(), cx));
        }
    }

    /// Reads the pull request again, unless it's merged or closed. An open one's checks, reviews and place in the
    /// merge queue change, and a workspace with none gets one when it's opened, by the agent or on GitHub.
    pub(crate) fn look_again_at_pull_request(&mut self, cx: &mut Context<Self>) {
        let done =
            self.pull_request.pull_request().is_some_and(|pr| matches!(pr.state, PrState::Merged | PrState::Closed));
        if !done {
            self.refresh_pull_request(cx);
        }
    }
}
