//! Where each reply's `result` is spelled out, as `pinned` has it for each request's `method`. The table is a `match`
//! with no arm for any reply but those it lists, so a reply added to `ClientResponse` doesn't build until it's given a
//! row, and its row fails until the file it names spells out the reply's `result`.

use std::path::Path;

use protocol::client::ClientResponse;

use super::pinned::method;

/// Makes `listed`, the match, and `PINNED`, each reply's variant and the file under `tests/compatibility/` that spells
/// out its `result`, from the same rows, so neither can leave out a reply the other has.
macro_rules! pinned {
    ($($file:literal => [$($variant:ident $(($fields:tt))?),* $(,)?]),* $(,)?) => {
        #[expect(dead_code, reason = "only built, so that a reply with no row in it doesn't build")]
        fn listed(reply: &ClientResponse) -> &'static str {
            match reply {
                $($(ClientResponse::$variant $(($fields))? => stringify!($variant),)*)*
            }
        }

        const PINNED: &[(&str, &str)] = &[$($((stringify!($variant), $file),)*)*];
    };
}

pinned! {
    "client_calls/devices.rs" => [Paired(_), Devices(_), Pairing(_), Device(_)],
    "client_calls/replies.rs" => [
        Hello(_), Ok, HubStatus(_), HubSettings(_), NewWorkspaceDraft(_), NewWorkspaceMade(_), Repo(_), Repos(_),
        Records(_), Agents(_), Workspace(_), Workspaces(_), GitStatus(_), GitDiff(_), SetupLog(_), PullRequest(_),
        Comment(_), Comments(_), Session(_), Sessions(_), Events(_), Subagents(_), Proposals(_), PlacedEvents(_),
        Prompts(_), Compactions(_), Found(_), PromptRecorded(_), Attachment(_), AttachmentData(_), SessionAccount(_),
        Account(_), GithubAccount(_), GithubSignIn(_), GhLogin(_), MachineLogins(_), WorktreeFiles(_), FileContents(_),
        FileOpened(_), FilePages(_),
    ],
    "client_calls/github_apps.rs" => [GithubApp(_), GithubAppManifest(_)],
    "replies_heard.rs" => [RepliesHeard(_)],
    "proposals.rs" => [Proposal(_), ProposalRevisions(_)],
    "references.rs" => [References(_)],
    "terminals.rs" => [Terminal(_), TerminalRows(_)],
    "terminal_text.rs" => [TerminalBounds(_), TerminalText(_)],
    "terminal_fit.rs" => [TerminalFit(_)],
    "model_lists.rs" => [ModelList(_)],
}

#[test]
fn every_reply_has_its_result_spelled_out_where_its_row_says() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compatibility");
    let mut missing = Vec::new();

    for &(variant, file) in PINNED {
        let result = method(variant);
        let spelled_out = std::fs::read_to_string(tests.join(file)).unwrap();
        if !spelled_out.contains(&format!(r#""result":"{result}""#)) {
            missing.push(format!("{variant} in {file}"));
        }
    }

    assert!(missing.is_empty(), "no reply's result spelled out as each row says: {missing:?}");
}
