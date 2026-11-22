//! The workspace's pull request: taking it as the hub sends it, and doing what its bar offers.

use gpui_kit::Context;

use super::{PaneEvent, WorkspacePane};
use crate::pull_request::{Action, Ask, Lookup};

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
        if let Some(pr) = self.pull_request.pull_request() {
            return pr.said().base_branch.as_str();
        }
        let from = self.workspace().base_ref.as_deref().unwrap_or("the default branch");
        from.trim_prefix("origin/")
    }

    /// Takes the pull request as the hub sent it, or as the workspace's record has it from a hub too old to send it,
    /// once the hub has said anything of it: looking where it said it hasn't looked, as once its cache has forgotten
    /// it. Until then the bar keeps what it showed, as between connecting and the hub's first word.
    pub(crate) fn take_pull_request(&mut self, cx: &mut Context<Self>) {
        let github = self.records.github();
        if github.said_pull_request(self.workspace()) {
            let lookup = Lookup::of(github.pull_request(self.workspace()));
            if lookup != self.pull_request {
                self.pull_request = lookup;
                cx.notify();
            }
        }
    }
}
