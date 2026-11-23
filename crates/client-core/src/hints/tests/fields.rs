//! Each field's hint words, in the order it takes them: what's typed, what it's about, then the workspace and the chat.

use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::fixtures;
use protocol::model::{ProposalOption, ProposalQuestion, Proposed, PullRequest};

use super::context::{sample_log, sample_workspace, texts};
use crate::hints::{Around, Field, Workspace};
use crate::records::InWorkspace;
use crate::session_log::SessionLog;

const CHANGED: &str = "crates/relay/src/tunnel.rs";

fn pull_request() -> PullRequest {
    let mut pull_request = fixtures::pull_request(7);
    pull_request.summary.said.about.title = "Keep the Ardmore relay up".into();
    pull_request
}

fn around<'a>(shown: &'a InWorkspace, pull_request: &'a PullRequest, chat: &'a SessionLog) -> Around<'a> {
    let workspace = Workspace { shown, pull_request: Some(pull_request), changed: vec![CHANGED] };
    Around { workspace: Some(workspace), chat: Some(chat) }
}

fn question() -> Question {
    Question {
        text: "Should `TunnelPool` keep idle links?".into(),
        header: Some("Pooling".into()),
        options: vec![
            QuestionOption { label: "Keep them".into(), description: Some("Uses `idle_ms`.".into()), preview: None },
            QuestionOption { label: "Drop them".into(), description: None, preview: None },
        ],
        choose: Choose::One,
        typed: TypedAnswer::Yes,
    }
}

fn proposal() -> Proposed {
    Proposed {
        title: "Holding the Ardmore link".into(),
        context: "Today `LinkKeeper` drops it at once.".into(),
        questions: vec![ProposalQuestion {
            header: "Grace".into(),
            text: "How long is it kept?".into(),
            details: String::new(),
            options: vec![ProposalOption {
                label: "Kesteven".into(),
                description: "A minute, in `grace_ms`.".into(),
                body: String::new(),
                letter: None,
            }],
            recommended: None,
        }],
    }
}

#[test]
fn the_composer_hints_what_s_typed_its_files_the_workspace_then_the_chat() {
    let (shown, chat, pull_request) = (sample_workspace(), sample_log(), pull_request());
    let attached = vec!["notes/Glenrothes.md".to_string()];

    let field = Field::Composer {
        typed: "Ask `Balquhidder` about it",
        attached: &attached,
        open_file: Some("crates/relay/src/handshake.rs"),
        around: around(&shown, &pull_request, &chat),
    };

    assert_eq!(
        texts(field.hint_words()),
        [
            "Balquhidder",
            "Glenrothes",
            "handshake",
            "relay",
            "entrecampos",
            "dictation-hints",
            "roundhouse",
            "Ardmore",
            "tunnel",
            "dictation",
            "cblas_sgemm",
            "Core ML",
            "Neural Engine",
            "Whisper"
        ]
    );
}

#[test]
fn an_answer_hints_what_s_typed_its_question_the_workspace_then_the_chat() {
    let (shown, chat, pull_request) = (sample_workspace(), sample_log(), pull_request());
    let question = question();

    let field = Field::Answer {
        typed: "Keep `Balquhidder`",
        question: &question,
        around: around(&shown, &pull_request, &chat),
    };

    assert_eq!(
        texts(field.hint_words()),
        [
            "Balquhidder",
            "Pooling",
            "idle_ms",
            "TunnelPool",
            "entrecampos",
            "dictation-hints",
            "roundhouse",
            "Ardmore",
            "tunnel",
            "relay",
            "dictation",
            "cblas_sgemm",
            "Core ML",
            "Neural Engine",
            "Whisper"
        ]
    );
}

#[test]
fn a_proposal_note_hints_what_s_typed_its_proposal_the_workspace_then_the_chat() {
    let (shown, chat, pull_request) = (sample_workspace(), sample_log(), pull_request());
    let proposal = proposal();

    let field = Field::ProposalNote {
        typed: Some("Ask `Balquhidder`"),
        proposal: &proposal,
        around: around(&shown, &pull_request, &chat),
    };

    assert_eq!(
        texts(field.hint_words()),
        [
            "Balquhidder",
            "Ardmore",
            "Grace",
            "Kesteven",
            "grace_ms",
            "LinkKeeper",
            "entrecampos",
            "dictation-hints",
            "roundhouse",
            "tunnel",
            "relay",
            "dictation",
            "cblas_sgemm",
            "Core ML",
            "Neural Engine",
            "Whisper"
        ]
    );
}

#[test]
fn the_new_workspace_dialog_hints_what_s_typed_its_files_then_the_repositories_chosen_first() {
    let attached = vec!["notes/Glenrothes.md".to_string()];

    let field = Field::NewWorkspace {
        typed: "Start on `Balquhidder`",
        attached: &attached,
        chosen_repo: Some("signalbox"),
        repos: vec!["roundhouse", "signalbox"],
    };

    assert_eq!(texts(field.hint_words()), ["Balquhidder", "Glenrothes", "signalbox", "roundhouse"]);
}

#[test]
fn a_diff_comment_hints_what_s_typed_its_file_then_the_changed_files() {
    let field = Field::DiffComment {
        typed: "Why `Balquhidder`?",
        path: "crates/relay/src/handshake.rs",
        changed: vec!["crates/relay/src/handshake.rs", CHANGED],
    };

    assert_eq!(texts(field.hint_words()), ["Balquhidder", "handshake", "relay", "tunnel"]);
}
