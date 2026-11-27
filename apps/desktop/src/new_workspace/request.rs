//! What the new-workspace dialog hands the app once Create has finished its prompt and draft.

use client_core::FileToSend;
use protocol::attachment::Attachment;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::model::{AgentKind, ModelName, NewWorkspaceDraftToKeep, RepoId};
use protocol::prompt::{Prompt, says_or_carries};

/// What Create asks the app to make.
#[derive(Debug)]
pub struct Request {
    pub repo: RepoId,
    /// What was typed and attached, which is sent once the workspace is made, unless it says nothing and carries
    /// nothing, as [`Request::has_prompt`] says. Its files are the ones the dialog had, to give back.
    pub prompt: Prompt<FileToSend>,
    /// Files the hub keeps for the dialog that hadn't been read back yet, which come before the prompt's.
    pub coming_back: Vec<Attachment>,
    /// The kind of agent its first chat runs.
    pub agent: AgentKind,
    /// The model and effort the dialog showed, or `None` for what the agent starts a chat with. With nothing picked,
    /// they're `None` when the hub refused to describe its agents or its reply couldn't be read, and until the agent
    /// has said what it offers.
    pub model: Option<ModelName>,
    pub effort: Option<Effort>,
    /// The agent's plan mode, if plan mode was on in the dialog, and otherwise `None`, for the one it starts a chat in.
    pub mode: Option<ApprovalMode>,
    /// What Create had the hub keep as what's in the dialog, which the hub makes the workspace from. It's what the
    /// dialog keeps, not what it shows, so its model and effort are `None` unless one was picked or kept, as the hub's
    /// copy says, and the hub can tell once the dialog says nothing else.
    pub made_of: NewWorkspaceDraftToKeep,
}

impl Request {
    /// A request that's not from the dialog, such as a test's, which asks the hub to make what it says, with the model
    /// and effort it names, or with `None`, what the agent starts a chat with. The hub refuses any file it carries,
    /// since only the dialog gives the hub its files.
    #[cfg(any(test, feature = "testing"))]
    pub fn new(
        repo: RepoId,
        prompt: Prompt<FileToSend>,
        agent: AgentKind,
        (model, effort): (Option<ModelName>, Option<Effort>),
        mode: Option<ApprovalMode>,
    ) -> Self {
        let made_of = NewWorkspaceDraftToKeep {
            agent: Some(agent),
            repo_id: Some(repo.clone()),
            text: prompt.text.clone(),
            files: prompt.files.iter().map(|file| file.attachment.id.clone()).collect(),
            model: model.clone(),
            effort: effort.clone(),
            mode,
        };
        let coming_back = Vec::new();
        Self { repo, prompt, coming_back, agent, model, effort, mode, made_of }
    }

    /// Whether there's a prompt to send once the workspace is made, counting the files still coming back.
    pub fn has_prompt(&self) -> bool {
        says_or_carries(&self.prompt.text, self.coming_back.len() + self.prompt.files.len())
    }
}
