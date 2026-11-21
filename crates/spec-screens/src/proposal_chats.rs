//! A chat whose agent proposed a decision too large for a question: where a train held at a signal keeps its wait. The
//! same made-up railway as the other chats.

use client_core::fake_hub::events::{first_turn, mcp, mcp_said, replied, text, user_prompt};
use protocol::event::EventBody;
use protocol::model::{
    OptionPlace, Proposal, ProposalOption, ProposalQuestion, ProposalRevision, Proposed, Recommendation, Response,
    RevisionNumber, Session,
};
use serde_json::json;

/// The call that proposed, which the proposal's record names.
pub const CALL: &str = "toolu-p1";
pub const ID: &str = "8d0c6a5e-proposal";

/// The proposal of the chat `session`, whose revisions are `revisions`, oldest first, each made as the chat was.
pub fn proposal(session: &Session, mut revisions: Vec<ProposalRevision>) -> Proposal {
    for revision in &mut revisions {
        revision.made_at_ms = session.created_at_ms;
    }
    Proposal {
        id: ID.into(),
        workspace_id: session.workspace_id.clone(),
        session_id: session.id.clone(),
        current: revisions.last().unwrap().clone(),
        revisions: revisions.iter().map(ProposalRevision::summary).collect(),
        withdrawn: None,
        created_at_ms: session.created_at_ms,
        rev: protocol::model::Rev::new(1),
    }
}

/// The first revision, which the person responded to with `response`, if they did.
pub fn first(response: Option<Response>) -> ProposalRevision {
    ProposalRevision {
        number: RevisionNumber::FIRST,
        changed: String::new(),
        call: Some(CALL.into()),
        proposed: proposed(),
        made_at_ms: 0,
        response,
        images: Vec::new(),
    }
}

/// The chat up to the agent's word on what it proposed.
pub fn events() -> Vec<EventBody> {
    first_turn(
        user_prompt("Trains should be able to wait at a signal for longer than a minute. Work out how first."),
        [
            text("m1", "I read how the controller holds a train at a signal, and how long it waits now."),
            text("m2", "Three ways would work, and they differ in where the wait is kept. I've written them up."),
            mcp("m3", CALL, "roundhouse", "propose", json!({ "title": "Holding a train" })),
            mcp_said(CALL, &format!("Proposed \"Holding a train\" as {ID}."), false),
            text("m4", "I'd go with A. Ask me anything about them here first, if you like."),
            replied("I'd go with A. Ask me anything about them here first, if you like.", 74, 0.41),
        ],
    )
}

/// What the agent proposed: three places a held train's wait could live.
pub fn proposed() -> Proposed {
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
