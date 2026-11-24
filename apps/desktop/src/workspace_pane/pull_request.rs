//! The workspace's pull request: taking it as the hub sends it, and doing what its bar offers.

use client_core::GithubFeed;
use gpui_kit::Context;
use protocol::model::Workspace;

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
        if let Some(pr) = self.pull_request.pull_request() {
            return pr.said().base_branch.as_str();
        }
        let from = self.workspace().base_ref.as_deref().unwrap_or("the default branch");
        from.trim_prefix("origin/")
    }

    /// The workspace's pull request and its warnings, as the hub last said them in `github`.
    pub(super) fn known(github: &GithubFeed, workspace: &Workspace) -> (Lookup, Vec<Warning>) {
        let unseen = github.unseen(workspace);
        let warnings = unseen.iter().map(Warning::unseen).collect();
        (Lookup::of(github.pull_request(workspace), unseen), warnings)
    }

    /// Takes the pull request as the hub sent it, once the hub has said anything of it: looking where it said it hasn't
    /// looked, as once its cache has forgotten it. Until then the bar keeps what it showed, as between connecting and
    /// the hub's first word.
    pub(crate) fn take_pull_request(&mut self, cx: &mut Context<Self>) {
        let github = self.records.github();
        if github.said_pull_request(self.workspace()) {
            let (lookup, warnings) = Self::known(&github, self.workspace());
            if lookup != self.pull_request || warnings != self.warnings {
                (self.pull_request, self.warnings) = (lookup, warnings);
                cx.notify();
            }
        }
    }
}
