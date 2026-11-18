//! What the new-workspace dialog hands the app once Create has finished its prompt and draft.

use client_core::FileToSend;
use protocol::attachment::Attachment;
use protocol::model::{AgentKind, Effort, ModelName, NewWorkspaceDraftToKeep, PermissionMode, RepoId};
use protocol::prompt::{Prompt, says_or_carries};

/// What Create asks the app to make.
#[derive(Debug)]
pub struct Request {
    pub repo: RepoId,
    /// What was typed and attached, which is sent once the workspace is made, unless it says nothing and carries
    /// nothing, as [`Request::has_prompt`] says. Its files are the ones the dialog had, to give back, or to send to a
    /// hub too old to make the workspace from what it keeps.
    pub prompt: Prompt<FileToSend>,
    /// Files the hub keeps for the dialog that hadn't been read back yet, which come before the prompt's.
    pub coming_back: Vec<Attachment>,
    /// The kind of agent its first chat runs.
    pub agent: AgentKind,
    /// The model and effort the dialog showed. `None` from a hub that doesn't describe its agents, with nothing picked,
    /// for what the agent starts a chat with.
    pub model: Option<ModelName>,
    pub effort: Option<Effort>,
    /// Plan mode, if it was on in the dialog, and full access otherwise.
    pub permission_mode: PermissionMode,
    /// What Create had the hub keep as what's in the dialog, which the hub makes the workspace from. It's what the
    /// dialog keeps, not what it shows, so its model and effort are `None` unless one was picked or kept, as the hub's
    /// copy says, and the hub can tell once the dialog says nothing else.
    pub made_of: NewWorkspaceDraftToKeep,
    /// Answers once the hub keeps `made_of` as what's in the dialog, or once that's as far as it can get, so the hub
    /// can make the workspace from what it keeps. `None` for a request that's not from the dialog.
    pub kept: Option<flume::Receiver<()>>,
}

impl Request {
    /// A request that's not from the dialog, such as a test's, which asks the hub to make what it says, with the model
    /// and effort it names, or with `None`, what the agent starts a chat with. The hub refuses any file it carries,
    /// since only the dialog gives the hub its files.
    #[cfg(feature = "testing")]
    pub fn new(
        repo: RepoId,
        prompt: Prompt<FileToSend>,
        agent: AgentKind,
        (model, effort): (Option<ModelName>, Option<Effort>),
        permission_mode: PermissionMode,
    ) -> Self {
        let made_of = NewWorkspaceDraftToKeep {
            agent: Some(agent),
            repo_id: Some(repo.clone()),
            text: prompt.text.clone(),
            files: prompt.files.iter().map(|file| file.attachment.id.clone()).collect(),
            model: model.clone(),
            effort: effort.clone(),
            plan: permission_mode == PermissionMode::Plan,
        };
        let coming_back = Vec::new();
        Self { repo, prompt, coming_back, agent, model, effort, permission_mode, made_of, kept: None }
    }

    /// Whether there's a prompt to send once the workspace is made, counting the files still coming back.
    pub fn has_prompt(&self) -> bool {
        says_or_carries(&self.prompt.text, self.coming_back.len() + self.prompt.files.len())
    }
}
