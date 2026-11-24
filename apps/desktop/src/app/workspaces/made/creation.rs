use crate::new_workspace::Request;
use client_core::{Client, ClientError, FileToSend};
use protocol::attachment::Attachment;
use protocol::capability::approvals::ApprovalMode;
use protocol::client::ApiError;
use protocol::client::new_workspace::{NewWorkspaceMade, UnsentPrompt};
use protocol::model::{Session, Workspace};
use protocol::prompt::Prompt;

/// How far making a workspace got.
pub(in super::super) struct Made {
    pub(in super::super) workspace: Option<Workspace>,
    /// Its first chat.
    pub(in super::super) session: Option<Session>,
    /// The prompt, if a step failed before it was sent.
    pub(in super::super) unsent: Option<Unsent>,
}

pub(in super::super) struct Unsent {
    /// What wasn't sent, to give back, or `None` once the hub has put it in the new chat's composer itself.
    pub(in super::super) given_back: Option<GivenBack>,
    /// The mode the chat was to start in, which goes back with the prompt.
    pub(in super::super) mode: Option<ApprovalMode>,
    pub(in super::super) failed: Step,
    pub(in super::super) error: ClientError,
}

/// A prompt that wasn't sent, for the dialog or the new chat's composer to put back.
pub(in super::super) struct GivenBack {
    pub(in super::super) prompt: Prompt<FileToSend>,
    /// Files the hub keeps for the dialog that weren't read back, which only the dialog can put back.
    pub(in super::super) coming_back: Vec<Attachment>,
    /// Whether the hub still keeps it as what's in the dialog, as one it couldn't give the new chat whole, so it goes
    /// back there, as one is when nothing was made, rather than to the new chat's composer.
    pub(in super::super) in_the_dialog: bool,
}

impl Unsent {
    /// All of `request`, given back, since `failed` failed before anything was sent.
    pub(super) fn of(request: Request, failed: Step, error: ClientError) -> Self {
        let Request { prompt, coming_back, mode, .. } = request;
        let given_back = Some(GivenBack { prompt, coming_back, in_the_dialog: false });
        Self { given_back, mode, failed, error }
    }
}

/// A step in making a workspace, each of which can fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Step {
    Workspace,
    /// Giving the chat the model, effort and mode chosen. The prompt isn't sent without them, so a chat that's to plan
    /// doesn't start on it with full access.
    Model,
    Prompt,
    /// Hearing back from the hub, which may have made it.
    Answer,
}

impl Step {
    /// What to say when this step failed.
    pub(in super::super) fn failed(self, e: &ClientError) -> String {
        match self {
            Step::Workspace => client_core::refusals::couldnt("create the workspace", e),
            Step::Model => client_core::refusals::couldnt("give the chat the model, effort and mode chosen", e),
            Step::Prompt => client_core::refusals::couldnt("send that", e),
            Step::Answer => client_core::refusals::couldnt("tell whether the workspace was made", e),
        }
    }
}

/// Has the hub make it from what it keeps of the dialog, once that's what's asked for. A refusal comes before anything
/// is made, so all of it is given back.
pub(in super::super) fn make(client: &Client, request: Request) -> Made {
    if let Some(kept) = &request.kept {
        // An answer either way, so a hub that's gone can't hold this forever.
        let _ = kept.recv();
    }
    match client.new_workspace_create(&request.made_of, client_core::new_request_id()) {
        Ok(NewWorkspaceMade { workspace, session, unchosen, unsent }) => {
            let unsent = unsent_by_the_hub(request, unchosen, unsent);
            Made { workspace: Some(workspace), session: Some(session), unsent }
        }
        Err(error) => {
            // A refusal comes before anything is made. Anything else, such as the connection going, may have come
            // after.
            let failed = if matches!(error, ClientError::Api(_)) { Step::Workspace } else { Step::Answer };
            let unsent = Some(Unsent::of(request, failed, error));
            Made { workspace: None, session: None, unsent }
        }
    }
}

/// What the hub says it didn't do with the prompt of a workspace it made. One in the new chat's composer is there
/// already. One the hub couldn't give the chat whole is still in the dialog, which is where it goes back, with the
/// files the dialog hadn't read back. A chat that couldn't be given what was chosen is said to have failed at that.
fn unsent_by_the_hub(request: Request, unchosen: Option<ApiError>, unsent: Option<UnsentPrompt>) -> Option<Unsent> {
    let failed = if unchosen.is_some() { Step::Model } else { Step::Prompt };
    let mode = request.mode;
    match (unsent, unchosen) {
        (Some(UnsentPrompt { error, in_composer: false }), unchosen) => {
            let mut unsent = Unsent::of(request, failed, ClientError::Api(unchosen.unwrap_or(error)));
            if let Some(given_back) = &mut unsent.given_back {
                given_back.in_the_dialog = true;
            }
            Some(unsent)
        }
        (Some(UnsentPrompt { error, in_composer: true }), unchosen) => {
            let error = ClientError::Api(unchosen.unwrap_or(error));
            Some(Unsent { given_back: None, mode, failed, error })
        }
        // Nothing was typed, so there's nothing to give back, but the chat still isn't as chosen.
        (None, Some(unchosen)) => Some(Unsent { given_back: None, mode, failed, error: ClientError::Api(unchosen) }),
        (None, None) => None,
    }
}
