//! The workspace's pull request: reading it as the hub sent it, and doing what its bar offers.

use gpui_kit::Context;
use protocol::model::Observed;

use super::{PaneEvent, WorkspacePane};
use crate::pull_request::{Action, Ask, Lookup};
use crate::warnings::Warning;

impl WorkspacePane {
    pub fn create_pull_request(&mut self, cx: &mut Context<Self>) {
        self.ask(Ask::CreatePr, cx);
    }

    /// Does what the pull request's bar offers.
    pub(crate) fn act(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Ask(ask) => self.ask(ask, cx),
            Action::Archive => cx.emit(PaneEvent::Archive(self.workspace().id.clone(), false)),
            Action::OpenGit => cx.emit(PaneEvent::OpenGit),
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
        if let Some(pr) = self.pull_request().pull_request() {
            return pr.summary.base_branch.as_str();
        }
        let from = self.workspace().base_ref.as_deref().unwrap_or("the default branch");
        from.trim_prefix("origin/")
    }

    /// The workspace's pull request, as the hub last said it in the records the pane holds.
    pub fn pull_request(&self) -> Lookup<'_> {
        let (records, id) = (self.shown.all(), &self.workspace().id);
        Lookup::of(records.workspace_pull_request(id), records.workspace_repository_unseen(id))
    }

    /// What's wrong that the workspace can't put right itself, which the card its name opens says, as the hub last
    /// said: GitHub showing none of the repository where its pull requests are.
    pub(super) fn warnings(&self) -> Vec<Warning> {
        match self.shown.all().workspace_repository_unseen(&self.workspace().id) {
            Observed::Current { value } | Observed::AsOf { value, .. } => value.iter().map(Warning::unseen).collect(),
            Observed::NotKnown | Observed::Unknown => Vec::new(),
        }
    }
}
