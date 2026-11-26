//! What a field's words were picked from, kept as a recording's file keeps it, gives those words again.

use hint_words::{PickedFrom, Places};
use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::client::HubNotice;
use protocol::event::EventBody;
use protocol::fixtures;
use protocol::model::{
    Branch, Branches, DiffComment, DiffSide, Head, OptionPlace, Place, ProposalOption, ProposalQuestion, Proposed,
    PullRequestSaid, Recommendation, Records, Repo, Rev, Seq,
};
use protocol::prompt::{Prompt, PromptFrom};

use super::context::{sample_log, texts};
use crate::hints::{Around, Field, Workspace, pick};
use crate::records::{HubRecords, InWorkspace};
use crate::session_log::SessionLog;

/// A workspace with everything that gives it a word: its branch, another checked out, one it had, its repository and
/// comments on two files.
fn workspace() -> InWorkspace {
    let mut workspace = fixtures::workspace("w1");
    workspace.name = "entrecampos".into();
    workspace.branch = "entrecampos/kelso".into();
    workspace.repo_id = "r1".into();
    let repo = Repo { name: "signalbox".into(), rev: Rev::new(1), ..fixtures::repo("r1") };
    let comment = |id: &str, path: &str, created_at_ms| DiffComment {
        id: id.into(),
        workspace_id: "w1".into(),
        path: path.into(),
        side: DiffSide::New,
        line: 1,
        text: String::new(),
        created_at_ms,
        rev: Rev::new(1),
    };
    let comments = vec![comment("c1", "Galashiels.rs", 1), comment("c2", "Peebles.rs", 2)];
    let branch = |name: &str| Branch { name: name.into(), tip: "0".repeat(40).as_str().into(), upstream: None };
    let branches = Branches {
        head: Head::Branch { name: "entrecampos/melrose".into() },
        had: vec![branch("entrecampos/hawick"), branch("entrecampos/melrose")],
    };
    let mut records = HubRecords::default();
    records.apply_list(Records { repos: vec![repo], workspaces: vec![workspace], comments, ..Records::default() });
    records.apply_notice(HubNotice::Observation(fixtures::observed::branches("w1", branches)));
    records.snapshot().in_workspace(&"w1".into()).unwrap()
}

fn pull_request() -> PullRequestSaid {
    let mut said = PullRequestSaid::of(&fixtures::pull_request(7));
    said.summary.about.title = "Keep the Ardmore relay up".into();
    said
}

fn question() -> Question {
    Question {
        text: "Should `TunnelPool` keep idle links?".into(),
        header: Some("Pooling".into()),
        options: vec![QuestionOption {
            label: "Dunkeld".into(),
            description: Some("Uses `idle_ms`.".into()),
            preview: Some("`preview_only`".into()),
        }],
        choose: Choose::One,
        typed: TypedAnswer::Yes,
    }
}

fn proposal() -> Proposed {
    let option = ProposalOption {
        label: "Kesteven".into(),
        description: "A minute, in `grace_ms`.".into(),
        body: "Kept in `LinkTable`.".into(),
        letter: None,
    };
    let question = ProposalQuestion {
        header: "Grace".into(),
        text: "How long does `LinkTimer` run?".into(),
        details: "Read by `LinkReader`.".into(),
        options: vec![option],
        recommended: Some(Recommendation { option: OptionPlace(0), why: "It survives `hub_restart`.".into() }),
    };
    Proposed {
        title: "Holding the Brechin link".into(),
        context: "Today `LinkKeeper` drops it.".into(),
        questions: vec![question],
    }
}

/// Checks that `field` picks each of `words`, and that what it picked from, read back from a file, gives the same
/// words with the chat and the words it'd pick without one.
fn gives_its_words_again(field: &Field<'_>, chat: Option<&SessionLog>, words: &[&str]) {
    let picked = field.picked();
    let picks = texts(picked.words.clone());
    for word in words {
        assert!(picks.iter().any(|pick| pick == word), "{word} in {picks:?}");
    }
    let kept = serde_json::to_string(&picked.from).unwrap();
    let kept: PickedFrom = serde_json::from_str(&kept).unwrap();
    assert_eq!(kept, picked.from);
    assert_eq!(pick(&kept, chat), picked.words);
}

const WORKSPACE: [&str; 9] =
    ["entrecampos", "kelso", "melrose", "hawick", "signalbox", "Peebles", "Galashiels", "Ardmore", "tunnel"];

#[test]
fn what_each_field_picked_from_gives_its_words_again() {
    let (shown, log, pull_request) = (workspace(), sample_log(), pull_request());
    let (question, proposal) = (question(), proposal());
    let attached = vec!["notes/Glenrothes.md".to_string()];
    for chat in [None, Some(&log)] {
        let around = || Around {
            workspace: Some(Workspace {
                shown: &shown,
                pull_request: Some(&pull_request),
                changed: vec!["crates/relay/src/tunnel.rs"],
            }),
            chat,
        };
        let composer = Field::Composer {
            typed: "Ask `Balquhidder`",
            attached: &attached,
            open_file: Some("crates/relay/src/handshake.rs"),
            around: around(),
        };
        gives_its_words_again(&composer, chat, &[&["Balquhidder", "Glenrothes", "handshake"], &WORKSPACE[..]].concat());
        if chat.is_some() {
            gives_its_words_again(&composer, chat, &["cblas_sgemm", "Core ML"]);
        }

        let answer = Field::Answer { typed: "Ask `Balquhidder`", question: &question, around: around() };
        let asked = ["Balquhidder", "Pooling", "TunnelPool", "Dunkeld", "idle_ms"];
        gives_its_words_again(&answer, chat, &[&asked, &WORKSPACE[..]].concat());

        let note = Field::ProposalNote { typed: Some("Ask `Balquhidder`"), proposal: &proposal, around: around() };
        let proposed = ["Balquhidder", "Brechin", "Grace", "LinkTimer", "Kesteven", "grace_ms"];
        let markdown = ["LinkKeeper", "LinkReader", "LinkTable", "hub_restart"];
        gives_its_words_again(&note, chat, &[&proposed, &markdown[..], &WORKSPACE[..]].concat());
    }
    let new = Field::NewWorkspace {
        typed: "Ask `Balquhidder`",
        attached: &attached,
        chosen_repo: Some("signalbox"),
        repos: vec!["ledger", "signalbox"],
    };
    gives_its_words_again(&new, None, &["Balquhidder", "Glenrothes", "signalbox", "ledger"]);
    let comment = Field::DiffComment { typed: "Ask `Balquhidder`", path: "Peebles.rs", changed: vec!["tunnel.rs"] };
    gives_its_words_again(&comment, None, &["Balquhidder", "Peebles", "tunnel"]);
}

/// The order words are picked in, which is the order they're kept in: the branches from its own, and the newest comment
/// first.
#[test]
fn a_workspaces_names_are_kept_in_the_order_theyre_picked() {
    let (shown, pull_request) = (workspace(), pull_request());
    let around = Around {
        workspace: Some(Workspace { shown: &shown, pull_request: Some(&pull_request), changed: Vec::new() }),
        chat: None,
    };
    let from = Field::Composer { typed: "", attached: &[], open_file: None, around }.picked().from;

    let owned = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
    let kept = hint_words::Workspace {
        name: "entrecampos".into(),
        branches: owned(&["entrecampos/kelso", "entrecampos/melrose", "entrecampos/hawick", "entrecampos/melrose"]),
        repo: Some("signalbox".into()),
        commented: owned(&["Peebles.rs", "Galashiels.rs"]),
        pull_request: Some("Keep the Ardmore relay up".into()),
    };
    assert_eq!(from.workspace, Some(kept));
}

/// A chat word can have an earlier word of the workspace's written as it's hinted, so the words are picked again with
/// the chat rather than added to.
#[test]
fn a_chats_word_still_spells_the_workspaces() {
    let shown = workspace();
    let around = Around {
        workspace: Some(Workspace { shown: &shown, pull_request: None, changed: vec!["stale-reports.rs"] }),
        chat: None,
    };
    let from = Field::Composer { typed: "", attached: &[], open_file: None, around }.picked().from;
    let mut log = SessionLog::new(Place::default());
    let said = EventBody::UserPrompt {
        client_msg_id: None,
        prompt: Prompt::new("Load `stale-reports` here"),
        from: PromptFrom::Person,
    };
    log.apply_event(fixtures::event(Seq::new(1), said));

    let spelled = |chat| {
        let words = pick(&from, chat);
        words.iter().find(|word| word.text == "stale-reports").map(|word| word.spelled)
    };
    assert_eq!((spelled(None), spelled(Some(&log))), (Some(false), Some(true)));
}

/// The hub has the chat, so what's kept says only which of it the app had, whose words these are.
#[test]
fn the_chats_loaded_events_are_kept_by_their_places() {
    let picked_with = |log: &SessionLog| {
        let around = Around { workspace: None, chat: Some(log) };
        Field::Composer { typed: "", attached: &[], open_file: None, around }.picked().from
    };
    let whole = sample_log();
    let last = whole.row_count() as u64;
    assert_eq!(picked_with(&whole), PickedFrom { chat: vec![Places { first: 1, last }], ..PickedFrom::default() });

    let body = || EventBody::AgentOther { kind: "k".into(), subagent: None };
    let mut later = SessionLog::new(Place::new(500));
    later.apply_event(fixtures::event(Seq::new(501), body()));
    let chat = picked_with(&later).chat;
    assert_eq!(chat, [Places { first: 501, last: 501 }], "an event before it that isn't loaded isn't said to be");
}
