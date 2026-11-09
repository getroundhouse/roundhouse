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
            Action::Archive => cx.emit(PaneEvent::Archive(self.workspace.id.clone(), false)),
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
        if let Some(base) = self.pull_request.pull_request().and_then(|pr| pr.base.as_deref()) {
            return base;
        }
        let from = self.workspace.base_ref.as_deref().unwrap_or("the default branch");
        from.strip_prefix("origin/").unwrap_or(from)
    }

    pub(super) fn refresh_pull_request(&mut self, cx: &mut Context<Self>) {
        self.pull_request_asks += 1;
        let (client, id, asked) = (self.client.clone(), self.workspace.id.clone(), self.pull_request_asks);
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { client.workspace_pull_request(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                if t.pull_request.answer(asked == t.pull_request_asks, found) {
                    cx.notify();
                }
            });
        })
        .detach();
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
