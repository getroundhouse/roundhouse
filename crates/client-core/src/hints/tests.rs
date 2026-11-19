use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::capability::calls::Call;
use protocol::capability::calls::call::Read;
use protocol::event::{ContentBlock, EventBody};
use protocol::fixtures;
use protocol::model::{
    OptionPlace, ProposalOption, ProposalQuestion, Proposed, Recommendation, Records, Repo, Rev, RevisionNumber, Seq,
};
use protocol::prompt::{Prompt, PromptFrom};

use super::HintCollector;
use crate::records::{HubRecords, InWorkspace};
use crate::session_log::SessionLog;

fn sample_workspace() -> InWorkspace {
    let mut records = HubRecords::default();
    let mut ws = fixtures::workspace("w1");
    ws.name = "entrecampos".to_string();
    ws.branch = "entrecampos/dictation-hints".into();
    ws.repo_id = "r1".into();
    let repo = Repo { name: "roundhouse".into(), rev: Rev::new(1), ..fixtures::repo("r1") };
    records.apply_list(Records { repos: vec![repo], workspaces: vec![ws], ..Records::default() });
    records.snapshot().in_workspace(&"w1".into()).unwrap()
}

fn sample_log() -> SessionLog {
    let mut log = SessionLog::new(Seq::new(0));
    log.apply_event(fixtures::event(
        Seq::new(1),
        EventBody::Assistant {
            message_id: None,
            subagent: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: "Runs on Core ML and the Neural Engine via `cblas_sgemm`, unlike Whisper or `MAX_HINTS`.".into(),
                images: Vec::new(),
            }],
            first_index: None,
        },
    ));
    for seq in 2..45 {
        let read = Read { path: "crates/dictation/src/hints/words.rs".into(), lines: None, pages: None };
        let block = ContentBlock::Call { id: format!("t{seq}").into(), call: Call::Read(read) };
        log.apply_event(fixtures::event(
            Seq::new(seq),
            EventBody::Assistant {
                message_id: None,
                subagent: None,
                model: None,
                blocks: vec![block],
                first_index: None,
            },
        ));
    }
    log
}

#[test]
fn collects_jargon_and_proper_nouns_across_tool_heavy_turns_while_skipping_common_english() {
    let in_ws = sample_workspace();
    let log = sample_log();
    let question = Question {
        text: "Should `WorkspacePane` use SQLite or Postgres?".into(),
        header: Some("Database".into()),
        options: vec![
            QuestionOption {
                label: "(Recommended) Postgres".into(),
                description: Some("Uses `sqlx`.".into()),
                preview: None,
            },
            QuestionOption { label: "Redis".into(), description: None, preview: None },
        ],
        choose: Choose::One,
        typed: TypedAnswer::Yes,
    };

    let mut collector = HintCollector::new();
    collector.add_question(&question);
    collector.add_workspace(&in_ws);
    collector.add_session_log(&log);
    let hints = collector.finish();

    for expected in [
        "Database",
        "Postgres",
        "sqlx",
        "Redis",
        "WorkspacePane",
        "SQLite",
        "entrecampos",
        "dictation-hints",
        "roundhouse",
        "dictation",
        "Core ML",
        "Neural Engine",
        "cblas_sgemm",
        "Whisper",
    ] {
        assert!(hints.iter().any(|h| h == expected), "missing {expected:?} in {hints:?}");
    }
    for unwanted in ["src", "crates", "words", "hints", "MAX_HINTS", "Should", "Runs"] {
        assert!(!hints.iter().any(|h| h.eq_ignore_ascii_case(unwanted)), "unexpected {unwanted:?} in {hints:?}");
    }
}

/// What the person said hints at what they'll say, and what the hub wrote for them, such as an answer's proposal ID,
/// doesn't.
#[test]
fn a_prompt_the_hub_wrote_gives_no_hints() {
    let mut log = SessionLog::new(Seq::new(0));
    let from = PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST };
    let told = Prompt::new("I chose B, Kesslerbahn Platform, in your proposal \"Holding\" (p).");
    log.apply_event(fixtures::event(Seq::new(1), EventBody::UserPrompt { client_msg_id: None, prompt: told, from }));
    let said = Prompt::new("Ask Wollstonecraft about it.");
    let person = EventBody::UserPrompt { client_msg_id: None, prompt: said, from: PromptFrom::Person };
    log.apply_event(fixtures::event(Seq::new(2), person));

    let mut collector = HintCollector::new();
    collector.add_session_log(&log);
    let hints = collector.finish();

    assert!(hints.iter().any(|h| h == "Wollstonecraft"), "the person's words: {hints:?}");
    assert!(!hints.iter().any(|h| h.contains("Kesslerbahn")), "not the hub's: {hints:?}");
}

/// A proposal's note hints at its names, and at the code its Markdown names, but not at its everyday words.
#[test]
fn a_proposal_gives_its_names_and_the_code_its_markdown_names() {
    let option = |label: &str, description: &str, body: &str| ProposalOption {
        label: label.into(),
        description: description.into(),
        body: body.into(),
        letter: None,
    };
    let proposed = Proposed {
        title: "Holding a train".into(),
        context: "Today `SignalBox` releases every train at once.".into(),
        questions: vec![ProposalQuestion {
            header: "Dwell".into(),
            text: "Where does the wait live?".into(),
            details: "| | Kesslerbahn | Wollstonecraft |\n|-|-|-|\n| Restart | kept | lost |".into(),
            options: vec![
                option("Kesslerbahn", "It counts down in `dwell_ms`.", "Keeps it in `signal_state`."),
                option("Wollstonecraft", "The train waits.", "Nothing new is stored."),
            ],
            recommended: Some(Recommendation { option: OptionPlace(0), why: "It survives `hub_restart`.".into() }),
        }],
    };

    let mut collector = HintCollector::new();
    collector.add_proposal(&proposed);
    let hints = collector.finish();

    for expected in ["Dwell", "Kesslerbahn", "Wollstonecraft", "dwell_ms", "SignalBox", "signal_state", "hub_restart"] {
        assert!(hints.iter().any(|h| h == expected), "missing {expected:?} in {hints:?}");
    }
    for unwanted in ["Where", "train", "wait", "Nothing"] {
        assert!(!hints.iter().any(|h| h.eq_ignore_ascii_case(unwanted)), "unexpected {unwanted:?} in {hints:?}");
    }
}

/// A long proposal's Markdown gives at most its share, so the names added after it, such as the workspace's, still fit.
#[test]
fn a_long_proposal_leaves_room_for_what_comes_after_it() {
    let identifiers: Vec<String> = (0..200).map(|n| format!("`zeta_{n}`")).collect();
    let proposed = Proposed {
        title: "Holding".into(),
        context: identifiers.join(" "),
        questions: vec![ProposalQuestion {
            header: "Dwell".into(),
            text: "Where?".into(),
            details: String::new(),
            options: vec![],
            recommended: None,
        }],
    };

    let mut collector = HintCollector::new();
    collector.add_proposal(&proposed);
    collector.add_name("Wollstonecraft");
    let hints = collector.finish();

    assert!(hints.iter().any(|h| h == "Wollstonecraft"), "room after it: {} hints", hints.len());
    assert!(hints.iter().filter(|h| h.starts_with("zeta_")).count() <= 40, "{hints:?}");
}
