//! Where each request's form is spelled out. The table is a `match` with no arm for any request but those it lists, so
//! a request added to `ClientRequest` doesn't build until it's given a row, and its row fails until the file it names
//! spells the request out.

use std::path::Path;

use protocol::client::ClientRequest;
use protocol::frame;

/// Makes `listed`, the match, and `PINNED`, each request's variant and the file under `tests/compatibility/` that
/// spells it out, from the same rows, so neither can leave out a request the other has.
macro_rules! pinned {
    ($($file:literal => [$($variant:ident $(($fields:tt))?),* $(,)?]),* $(,)?) => {
        #[expect(dead_code, reason = "only built, so that a request with no row in it doesn't build")]
        fn listed(request: &ClientRequest) -> &'static str {
            match request {
                $($(ClientRequest::$variant $(($fields))? => stringify!($variant),)*)*
                // What a newer client asks, which `a_request_from_a_newer_client_still_decodes_with_its_id` pins.
                ClientRequest::Unknown => "Unknown",
            }
        }

        const PINNED: &[(&str, &str)] = &[$($((stringify!($variant), $file),)*)*];
    };
}

pinned! {
    "client_calls/devices.rs" => [Pair(_), Authenticate(_), PairingStart, PairingWait(_), DeviceList, DeviceRevoke(_)],
    "client_calls/requests.rs" => [
        Hello(_), HubStatus, HubShutdown, HubShutdownIfIdle, HubRestart, HubUpdateTo(_), HubConfigure(_), RepoAdd(_),
        RepoList,
        RecordsList, AgentList, ModelsFor(_), SessionSubagents(_), SessionProposals(_), AccountConnect(_),
        AccountSetDefault(_), AccountSetAutomatic(_), AccountDisconnect(_), AccountSetReserve(_),
        GithubAccountConnect(_), GithubAccountDisconnect(_),
        GithubAccountSetDefault(_), GithubSignInStart(_), GithubSignInCancel(_), HubGhLogin, HubMachineLogins,
        GithubAccountConnectGh,
        WorkspaceCreate(_), WorkspaceList(_),
        WorkspaceArchive(_), WorkspaceRestore(_), WorkspaceRename(_), WorkspaceRegenerateTitle(_), WorkspaceStatus(_),
        WorkspaceDiff(_), WorkspaceSetupLog(_), WorkspaceRerunSetup(_), WorkspacePullRequest(_),
        WorkspaceCommentDraft(_), CommentAdd(_), CommentList(_), CommentEdit(_), CommentDelete(_), WorkspaceFiles(_),
        WorkspaceFolderRead(_), WorkspaceFileRead(_), WorkspaceFileReadAt(_), WorkspaceFileOpenAt(_),
        WorkspaceFilePages(_),
    ],
    "client_calls/session_requests.rs" => [
        SessionCreate(_), SessionEnsure(_), SessionList(_), SessionGet(_), SessionConfigure(_), SessionClose(_),
        SessionReopen(_), SessionMarkRead(_), SessionMarkUnread(_), SessionConversationStart(_),
        SessionConversationEnd(_), SessionRename(_), SessionAccount(_),
        SessionCallOffWait(_), SessionAssignAccount(_), SessionRead(_), SessionPromptList(_), SessionCompactionList(_),
        SessionFind(_), SessionReadPlaces(_), SessionSubscribe(_), SessionSubscribePlaces(_), SessionUnsubscribe(_),
        FeedSubscribe(_), SessionPrompt(_), SessionQueue(_), SessionChangeQueued(_), SessionUnqueue(_),
        SessionSendQueued(_), SessionDecide(_), SessionCancel(_), SessionCompact(_), AttachmentAdd(_),
        AttachmentRead(_), AttachmentRemove(_), SessionDraft(_), SessionDecisionDraft(_), NewWorkspaceDraftKeep(_),
        NewWorkspaceDraftFileAdd(_), NewWorkspaceDraftFileRead(_), NewWorkspaceDraftFileRemove(_),
        NewWorkspaceCreate(_),
    ],
    "client_calls/github_apps.rs" => [
        GithubAppManifestWrite(_), GithubAppMake(_), GithubAppInstallationsConnect(_), GithubAppRemove(_),
    ],
    "replies_heard.rs" => [SessionReplyHeard(_), SessionRepliesHeard(_), SessionSpeaking(_)],
    "proposals.rs" => [ProposalAnswer(_), ProposalAnotherRound(_), ProposalRevisionsRead(_)],
    "references.rs" => [ReferencesRead(_)],
    "terminals.rs" => [TerminalOpen(_), TerminalAttach(_), TerminalInput(_), TerminalResize(_), TerminalClose(_)],
}

/// A request's `method`, or a reply's `result`, as serde's `snake_case` makes it from the variant's name.
pub(super) fn method(variant: &str) -> String {
    let mut method = String::new();
    for (at, letter) in variant.char_indices() {
        if letter.is_uppercase() && at > 0 {
            method.push('_');
        }
        method.push(letter.to_ascii_lowercase());
    }
    method
}

#[test]
fn every_request_has_its_form_spelled_out_where_its_row_says() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compatibility");
    let mut missing = Vec::new();

    for &(variant, file) in PINNED {
        let method = method(variant);
        let known = frame::decode::<ClientRequest>(format!(r#"{{"method":"{method}"}}"#).as_bytes());
        assert!(!matches!(known, Ok(ClientRequest::Unknown)), "{variant} isn't `{method}` on the wire");
        let spelled_out = std::fs::read_to_string(tests.join(file)).unwrap();
        if !spelled_out.contains(&format!(r#""method":"{method}""#)) {
            missing.push(format!("{variant} in {file}"));
        }
    }

    assert!(missing.is_empty(), "no request spelled out as each row says: {missing:?}");
}
