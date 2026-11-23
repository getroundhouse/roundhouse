//! What the collector keeps from each source, and what it leaves out.

use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use protocol::event::EventBody;
use protocol::fixtures;
use protocol::model::{OptionPlace, ProposalOption, ProposalQuestion, Proposed, Recommendation, RevisionNumber, Seq};
use protocol::prompt::{Prompt, PromptFrom};

use super::context::{sample_log, sample_workspace};
use crate::hints::HintCollector;
use crate::session_log::SessionLog;

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
