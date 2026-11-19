//! A chat whose agent proposed a decision too large for a question, and the screens about its line, the proposal tab
//! and its answer. The same made-up railway as the other chats.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::{
    Choice, OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalQuestion, ProposalRevision, Proposed,
    Recommendation, Response, RevisionNumber, SessionStatus,
};
use serde_json::json;

use super::frame::{CHAT, PROPOSAL};
use super::screens::FOCUS;
use super::steps::{DictateWith, Screen, Step};
use super::worlds::chat_world;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{mcp, mcp_said, replied, taken, text, user_prompt};

/// The call that proposed, which the proposal's record names.
const CALL: &str = "toolu-p1";
const ID: &str = "8d0c6a5e-proposal";

/// The screens about proposals.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "proposal-line",
        spec: "proposals.md",
        world: Some(world),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "proposal-tab",
        spec: "proposals.md",
        world: Some(world),
        frame: PROPOSAL,
        steps: &[Step::Select(FOCUS), Step::Click("open-proposal/8d0c6a5e-proposal"), Step::MoveAway],
    },
    Screen {
        name: "proposal-choosing",
        spec: "proposals.md",
        world: Some(world),
        frame: PROPOSAL,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("open-proposal/8d0c6a5e-proposal"),
            Step::Click("choose-0-0"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "proposal-dictation",
        spec: "proposals.md",
        world: Some(world),
        frame: PROPOSAL,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("open-proposal/8d0c6a5e-proposal"),
            Step::Click("choose-0-0"),
            Step::Click("input-state"),
            Step::Type("The signal, as long as"),
            Step::Dictate {
                with: DictateWith::Keys,
                said: "its count survives a restart of the hub",
                shows: "its count survives a restart of the",
            },
            Step::MoveAway,
        ],
    },
    Screen {
        name: "proposal-answered",
        spec: "proposals.md",
        world: Some(answered_world),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "proposal-revised-line",
        spec: "proposals.md",
        world: Some(revised_world),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "proposal-revised",
        spec: "proposals.md",
        world: Some(revised_world),
        frame: PROPOSAL,
        steps: &[Step::Select(FOCUS), Step::Click("open-proposal/8d0c6a5e-proposal/2"), Step::MoveAway],
    },
];

/// The call that wrote the second revision.
const REVISED: &str = "toolu-p2";

/// The prompt that told the agent what was chosen, by its message ID, which the proposal's answer names.
const TOLD: &str = "answer-1";

fn world() -> World {
    with_proposal(events(), [first(None)])
}

/// After the person asked in the chat for an option the proposal didn't have, and the agent revised it: B dropped, C
/// keeping its letter, and D added.
fn revised_world() -> World {
    let mut events = events();
    events.extend([
        user_prompt("None of these: a hold has to survive a restart, and I'd rather the signal kept no state. Could the dispatcher's board own it?"),
        taken(protocol::model::Seq::new(9)),
        text("m5", "I'll drop B, which can't survive a restart, and write up the dispatcher's board."),
        mcp("m6", REVISED, "roundhouse", "revise_proposal", json!({ "title": "Holding a train" })),
        mcp_said(REVISED, &format!("Revised \"Holding a train\" as revision 2 of {ID}."), false),
        replied("I'll drop B, which can't survive a restart, and write up the dispatcher's board.", 63, 0.31),
    ]);
    let first = first(None);
    let mut proposed = proposed();
    let question = &mut proposed.questions[0];
    let board = ProposalOption {
        label: "The dispatcher's board".into(),
        description: "The board that sets holds keeps them, and tells the signal.".into(),
        body: "Holds are kept where dispatch sets them, and survive a restart, as the board's state does.".into(),
        letter: OptionLetter::read("D"),
    };
    question.options.remove(1);
    question.options[1].letter = OptionLetter::read("C");
    question.options.push(board);
    question.details = "| | A · The signal | C · The timetable | D · The dispatcher's board |\n|---|---|---|---|\n\
                        | Survives a restart | Yes | Yes | Yes |\n| New state | One field | One table | One field |\n\
                        | Work | About a day | About three days | About two days |"
        .into();
    question.recommended = Some(Recommendation {
        option: OptionPlace(2),
        why: "It keeps the signal free of state, as you asked, and survives a restart.".into(),
    });
    let second = ProposalRevision {
        number: RevisionNumber(2),
        changed:
            "Dropped B, the train, which loses a hold on a restart. Added D, the dispatcher's board, as you asked. \
                  A and C are as they were."
                .into(),
        call: Some(REVISED.into()),
        proposed,
        ..first.clone()
    };
    with_proposal(events, vec![first, second])
}

/// The first revision, which the person answered with `choice`, if they did.
fn first(choice: Option<Choice>) -> ProposalRevision {
    ProposalRevision {
        number: RevisionNumber::FIRST,
        changed: String::new(),
        call: Some(CALL.into()),
        proposed: proposed(),
        made_at_ms: 0,
        response: choice.map(Response::Chose),
        images: Vec::new(),
    }
}

/// After the person chose A, with a note, and the agent was told.
fn answered_world() -> World {
    let mut events = events();
    let told = format!(
        "I chose A, The signal, in your proposal \"Holding a train\" ({ID}). I wrote a note with it, which get_proposal reads."
    );
    events.extend([
        EventBody::UserPrompt {
            client_msg_id: Some(TOLD.into()),
            prompt: protocol::prompt::Prompt::new(told),
            from: protocol::prompt::PromptFrom::ProposalResponse {
                proposal: ID.into(),
                revision: RevisionNumber::FIRST,
            },
        },
        taken(protocol::model::Seq::new(9)),
        text(
            "m5",
            "Going with the signal. I'll keep the countdown on `Signal`, and say in the PR that it's one field.",
        ),
        replied("Going with the signal.", 41, 0.22),
    ]);
    let answered =
        Choice { chosen: vec![OptionPlace(0)], note: "Hold it while the platform clears too.".into(), at_ms: 0 };
    with_proposal(events, [first(Some(answered))])
}

/// The world of the chat `events`, with the proposal whose revisions are `revisions`, oldest first.
fn with_proposal(events: Vec<EventBody>, revisions: impl Into<Vec<ProposalRevision>>) -> World {
    let mut world = chat_world(SessionStatus::Idle, events);
    let session = world.sessions.last().unwrap();
    let mut revisions = revisions.into();
    for revision in &mut revisions {
        revision.made_at_ms = session.created_at_ms;
    }
    let proposal = Proposal {
        id: ID.into(),
        workspace_id: session.workspace_id.clone(),
        session_id: session.id.clone(),
        current: revisions.last().unwrap().clone(),
        revisions: revisions.iter().map(ProposalRevision::summary).collect(),
        created_at_ms: session.created_at_ms,
        rev: protocol::model::Rev::new(1),
    };
    world.proposals.insert(session.id.clone(), vec![proposal]);
    world
}

fn events() -> Vec<EventBody> {
    vec![
        user_prompt("Trains should be able to wait at a signal for longer than a minute. Work out how first."),
        taken(protocol::model::Seq::new(1)),
        text("m1", "I read how the controller holds a train at a signal, and how long it waits now."),
        text("m2", "Three ways would work, and they differ in where the wait is kept. I've written them up."),
        mcp("m3", CALL, "roundhouse", "propose", json!({ "title": "Holding a train" })),
        mcp_said(CALL, &format!("Proposed \"Holding a train\" as {ID}."), false),
        text("m4", "I'd go with A. Ask me anything about them here first, if you like."),
        replied("I'd go with A. Ask me anything about them here first, if you like.", 74, 0.41),
    ]
}

fn proposed() -> Proposed {
    let option = |label: &str, description: &str, body: &str| ProposalOption {
        label: label.into(),
        description: description.into(),
        body: body.into(),
        letter: None,
    };
    let question = ProposalQuestion {
        header: "Holding".into(),
        text: "Where does a held train's wait live?".into(),
        details: "| | A · The signal | B · The train | C · The timetable |\n|---|---|---|---|\n\
                  | Survives a restart | Yes | No | Yes |\n| New state | One field | None | One table |\n\
                  | Work | About a day | Half a day | About three days |"
            .into(),
        options: vec![
            option(
                "The signal",
                "Each signal keeps how long it's held its train.",
                "The signal counts down, and lets the train go when it reaches zero.\n\n\
                 - **Costs:** one field on `Signal`, written as it changes.\n- **Risk:** none found.",
            ),
            option("The train", "Each train remembers how long it's been waiting.", "Nothing new is kept."),
            option("The timetable", "Holds go in the timetable, beside the stops.", "A table of holds, by stop."),
        ],
        recommended: Some(Recommendation {
            option: OptionPlace(0),
            why: "It's the smallest change that still holds a train across a restart.".into(),
        }),
    };
    Proposed {
        title: "Holding a train".into(),
        context: "A train held at a signal is let go after a minute, whatever the signal says. Dispatch wants to hold \
                  one for longer, such as while a platform is cleared."
            .into(),
        questions: vec![question],
    }
}
