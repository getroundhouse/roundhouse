//! A chat whose agent proposed a decision too large for a question, and the screens about its line, the proposal tab
//! and its answer. The same made-up railway as the other chats.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::{
    Choice, OptionPlace, Proposal, ProposalOption, ProposalQuestion, ProposalRevision, Proposed, Recommendation,
    Response, RevisionNumber, SessionStatus,
};
use serde_json::json;

use super::frame::{CHAT, PROPOSAL};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
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
        name: "proposal-answered",
        spec: "proposals.md",
        world: Some(answered_world),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
];

/// The prompt that told the agent what was chosen, by its message ID, which the proposal's answer names.
const TOLD: &str = "answer-1";

fn world() -> World {
    with_proposal(events(), None)
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
    with_proposal(events, Some(answered))
}

fn with_proposal(events: Vec<EventBody>, choice: Option<Choice>) -> World {
    let mut world = chat_world(SessionStatus::Idle, events);
    let session = world.sessions.last().unwrap();
    let proposal = Proposal {
        id: ID.into(),
        workspace_id: session.workspace_id.clone(),
        session_id: session.id.clone(),
        current: ProposalRevision {
            number: RevisionNumber::FIRST,
            call: Some(CALL.into()),
            proposed: proposed(),
            made_at_ms: session.created_at_ms,
            response: choice.map(Response::Chose),
            images: Vec::new(),
        },
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
