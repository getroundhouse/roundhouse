//! The proposal tools, on the current version of MCP.

use protocol::attachment::MediaType;
use protocol::model::ToolUseId;
use protocol::worker::{AskReply, ProposalRecord, Propose, WorkerAsk};
use serde_json::json;

use super::super::proposals;
use super::current::{answered, answered_by, asks, caller_in, headers, hub_answers, offering_tasks};

/// So the hub makes one proposal for the call however often it's asked, as each new hub is asked what the last
/// didn't answer, and the chat draws the proposal's line for that call.
#[tokio::test]
async fn a_proposal_is_asked_for_by_the_call_that_made_it() {
    let (asks, mut sent) = asks();
    let arguments = json!({ "title": "Search", "context": "", "questions": [{ "header": "Index",
        "question": "Where?", "options": [{ "label": "Hub", "description": "", "body": "" },
        { "label": "Client", "description": "", "body": "" }] }] });
    let call = offering_tasks("tools/call", &json!({ "name": proposals::PROPOSE, "arguments": arguments }));
    let headers = headers("tools/call", Some(proposals::PROPOSE));
    let calling = answered(&asks, &headers, &call);
    let made = protocol::fixtures::proposal("p1");
    let hub = hub_answers(&asks, &mut sent, Ok(AskReply::Proposal(ProposalRecord { proposal: made })));

    let (answer, asked) = tokio::join!(calling, hub);

    let WorkerAsk::Propose(Propose { proposed, call, key, .. }) = asked else { panic!("asked to propose: {asked:?}") };
    assert_eq!((call.as_ref().map(ToolUseId::as_str), key.as_str()), (Some("toolu_1"), "toolu_1"));
    assert_eq!(proposed.questions[0].options[1].label, "Client");
    let text = answer.to_json()["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(text.starts_with("Proposed \"Search\" as p1."), "{text}");
}

/// What the agent wrote points to images in its worktree, which go to the hub with the proposal, as they are now, each
/// named once by its address however many parts point to it, so the proposal keeps what it showed.
#[tokio::test]
async fn a_proposal_carries_the_images_it_points_to_in_the_worktree() {
    let worktree = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(worktree.path().join("docs")).unwrap();
    std::fs::write(worktree.path().join("docs/flow.svg"), b"<svg width=\"4\" height=\"4\"/>").unwrap();
    std::fs::write(worktree.path().join("now.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
    let (asks, mut sent) = asks();
    let arguments = json!({ "title": "Search", "context": "As it flows:\n\n![Flow](docs/flow.svg)",
        "questions": [{ "header": "Index", "question": "Where?", "options": [
            { "label": "Hub", "description": "", "body": "![Now](now.png) and ![Flow again](docs/flow.svg)" },
            { "label": "Client", "description": "", "body": "![Missing](gone.png)" }] }] });
    let call = offering_tasks("tools/call", &json!({ "name": proposals::PROPOSE, "arguments": arguments }));
    let (caller, headers) = (caller_in(worktree.path()), headers("tools/call", Some(proposals::PROPOSE)));
    let calling = answered_by(&asks, &caller, &headers, &call);
    let made = protocol::fixtures::proposal("p1");
    let hub = hub_answers(&asks, &mut sent, Ok(AskReply::Proposal(ProposalRecord { proposal: made })));

    let (_, asked) = tokio::join!(calling, hub);

    let WorkerAsk::Propose(Propose { images, attachments, .. }) = asked else { panic!("asked to propose: {asked:?}") };
    let named: Vec<_> = images.iter().map(|image| image.address.as_str()).collect();
    assert_eq!(named, ["docs/flow.svg", "now.png"], "each once, in the order they're read, and not the one missing");
    let kinds: Vec<_> = attachments.iter().map(|sent| sent.attachment.media_type).collect();
    assert_eq!(kinds, [MediaType::Svg, MediaType::Png]);
    assert_eq!(images[0].attachment, attachments[0].attachment, "each named by the bytes sent beside it");
}
