//! What a worker asks its hub to propose for its agent, and to read back, the hub's answer, and the notice that says
//! a chat has one, and a client's answer to one, or request for another round, and its read of one's revisions, which
//! every peer from now on reads as they were first written.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, NewAttachment};
use protocol::client::proposals::{ProposalAnotherRound, ProposalAnswer, ProposalRevisions, ProposalRevisionsRead};
use protocol::client::{ClientRequest, ClientResponse, HubNotice};
use protocol::event::LinkedImage;
use protocol::model::{
    Choice, ChosenOption, OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalQuestion, ProposalRevision,
    Proposed, Recommendation, Response, ResponseSummary, Rev, RevisionNumber, RevisionSummary, Round, Withdrawal,
};
use protocol::worker::{Ask as _, AskReply, ProposalRecord, Propose, ReadProposal, Revise, Withdraw, WorkerAsk};

use super::keeps_its_form;

const PROPOSED: &str = r#"{"title":"Search","context":"Why","questions":[{"header":"Index","text":"Where?","details":"","options":[{"label":"Hub","description":"One","body":"**b**"},{"label":"Client","description":"","body":""}],"recommended":{"option":0,"why":"Fewer"}}]}"#;

fn proposed() -> Proposed {
    let option = |label: &str, description: &str, body: &str| ProposalOption {
        label: label.into(),
        description: description.into(),
        body: body.into(),
        letter: None,
    };
    let question = ProposalQuestion {
        header: "Index".into(),
        text: "Where?".into(),
        details: String::new(),
        options: vec![option("Hub", "One", "**b**"), option("Client", "", "")],
        recommended: Some(Recommendation { option: OptionPlace(0), why: "Fewer".into() }),
    };
    Proposed { title: "Search".into(), context: "Why".into(), questions: vec![question] }
}

fn revision() -> (ProposalRevision, String) {
    let revision = ProposalRevision {
        number: RevisionNumber::FIRST,
        changed: String::new(),
        call: Some("toolu_1".into()),
        proposed: proposed(),
        made_at_ms: 1,
        response: None,
        images: Vec::new(),
    };
    (revision, format!(r#"{{"number":1,"call":"toolu_1","proposed":{PROPOSED},"made_at_ms":1}}"#))
}

fn proposal() -> (Proposal, String) {
    let (current, revision_form) = revision();
    let proposal = Proposal {
        id: "p".into(),
        workspace_id: "w".into(),
        session_id: "s".into(),
        current,
        revisions: Vec::new(),
        withdrawn: None,
        created_at_ms: 1,
        rev: Rev::new(2),
    };
    let form =
        format!(r#""id":"p","workspace_id":"w","session_id":"s","current":{revision_form},"created_at_ms":1,"rev":2"#);
    (proposal, form)
}

#[test]
fn proposing_and_reading_a_proposal_back_keep_their_form() {
    let (proposal, form) = proposal();
    let propose = Propose {
        proposed: proposed(),
        call: Some("toolu_1".into()),
        key: "toolu_1".into(),
        images: Vec::new(),
        attachments: Vec::new(),
    };

    keeps_its_form(
        &propose.into_ask(),
        &format!(r#"{{"method":"propose","proposed":{PROPOSED},"call":"toolu_1","key":"toolu_1"}}"#),
    );
    keeps_its_form(
        &ReadProposal { id: "p".into(), revision: None }.into_ask(),
        r#"{"method":"read_proposal","id":"p"}"#,
    );
    keeps_its_form(
        &AskReply::Proposal(ProposalRecord { proposal, revision: None }),
        &format!(r#"{{"result":"proposal","proposal":{{{form}}}}}"#),
    );
}

/// The images what the agent wrote points to: named on the ask, with their bytes beside them, and on the revision the
/// hub keeps, by the address the agent gave each, as a reply's text names them.
#[test]
fn the_images_a_proposal_points_to_keep_their_form() {
    let attachment = Attachment { id: "a1".into(), name: "flow.svg".into(), media_type: MediaType::Svg, size_bytes: 6 };
    let images = vec![LinkedImage { address: "docs/flow.svg".into(), attachment: attachment.clone() }];
    let bytes = NewAttachment { attachment, data: AttachmentBytes::new(b"<svg/>".to_vec()) };
    let propose =
        Propose { proposed: proposed(), call: None, key: "k".into(), images: images.clone(), attachments: vec![bytes] };
    let image = r#"{"address":"docs/flow.svg","attachment":{"id":"a1","name":"flow.svg","media_type":"image/svg+xml","size_bytes":6}}"#;
    let (revision, _) = revision();
    let revision = ProposalRevision { images, ..revision };

    keeps_its_form(
        &propose.into_ask(),
        &format!(
            r#"{{"method":"propose","proposed":{PROPOSED},"key":"k","images":[{image}],"attachments":[{{"attachment":{{"id":"a1","name":"flow.svg","media_type":"image/svg+xml","size_bytes":6}},"data":"PHN2Zy8+"}}]}}"#
        ),
    );
    keeps_its_form(
        &revision,
        &format!(r#"{{"number":1,"call":"toolu_1","proposed":{PROPOSED},"made_at_ms":1,"images":[{image}]}}"#),
    );
}

/// So a worker's agent can leave out what it has nothing to say in, and the call when its CLI doesn't name it.
#[test]
fn a_proposal_written_without_what_can_be_left_out_reads_with_it_empty() {
    let written = r#"{"method":"propose","proposed":{"title":"T","questions":[{"header":"H","text":"Q","options":[{"label":"A"},{"label":"B"}]}]},"key":"k"}"#;

    let WorkerAsk::Propose(read) = serde_json::from_str(written).unwrap() else { panic!("{written}") };

    let question = &read.proposed.questions[0];
    assert_eq!((read.call, read.proposed.context.as_str(), question.details.as_str()), (None, "", ""));
    assert_eq!((question.options[1].description.as_str(), question.options[1].body.as_str()), ("", ""));
    assert_eq!(question.recommended, None);
}

#[test]
fn the_notice_of_a_proposal_keeps_its_form() {
    let (proposal, form) = proposal();

    keeps_its_form(&HubNotice::Proposal(proposal), &format!(r#"{{"notice":"proposal",{form}}}"#));
}

#[test]
fn answering_a_proposal_and_the_answered_proposal_keep_their_form() {
    let answer = ProposalAnswer {
        id: "p".into(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(1)],
        note: "Keep it small".into(),
        client_request_id: "r".into(),
    };
    keeps_its_form(
        &ClientRequest::ProposalAnswer(answer),
        r#"{"method":"proposal_answer","id":"p","revision":1,"chosen":[1],"note":"Keep it small","client_request_id":"r"}"#,
    );

    let (proposal, form) = proposal();
    let answered = Choice { chosen: vec![OptionPlace(1)], note: String::new(), at_ms: 3 };
    let proposal = Proposal {
        current: ProposalRevision { response: Some(Response::Chose(answered)), ..proposal.current },
        ..proposal
    };
    let form =
        form.replace(r#""made_at_ms":1}"#, r#""made_at_ms":1,"response":{"kind":"chose","chosen":[1],"at_ms":3}}"#);
    keeps_its_form(&ClientResponse::Proposal(proposal), &format!(r#"{{"result":"proposal",{form}}}"#));
}

#[test]
fn reading_a_proposals_revisions_and_the_revisions_keep_their_form() {
    let read = ProposalRevisionsRead { id: "p".into(), from: RevisionNumber::FIRST, limit: 8 };
    keeps_its_form(
        &ClientRequest::ProposalRevisionsRead(read),
        r#"{"method":"proposal_revisions_read","id":"p","from":1,"limit":8}"#,
    );

    let (revision, form) = revision();
    keeps_its_form(
        &ClientResponse::ProposalRevisions(ProposalRevisions { revisions: vec![revision], newest: RevisionNumber(2) }),
        &format!(r#"{{"result":"proposal_revisions","revisions":[{form}],"newest":2}}"#),
    );
}

/// Asking for another round, the response the hub keeps for it, and the record's summary of it.
#[test]
fn another_round_and_its_response_keep_their_form() {
    let round = ProposalAnotherRound {
        id: "p".into(),
        revision: RevisionNumber::FIRST,
        note: "None of these".into(),
        client_request_id: "r".into(),
    };
    keeps_its_form(
        &ClientRequest::ProposalAnotherRound(round),
        r#"{"method":"proposal_another_round","id":"p","revision":1,"note":"None of these","client_request_id":"r"}"#,
    );
    keeps_its_form(
        &Response::AnotherRound(Round { note: "None of these".into(), at_ms: 3 }),
        r#"{"kind":"another_round","note":"None of these","at_ms":3}"#,
    );
    keeps_its_form(
        &ResponseSummary::AnotherRound { note: "None of".into(), at_ms: 3 },
        r#"{"kind":"another_round","note":"None of","at_ms":3}"#,
    );
}

/// The agent's withdrawal: the ask, and the record that says it's withdrawn, which a peer from before leaves out.
#[test]
fn withdrawing_a_proposal_and_its_record_keep_their_form() {
    let withdraw = Withdraw { id: "p".into(), why: "Decided".into(), call: Some("toolu_3".into()) };
    keeps_its_form(&withdraw.into_ask(), r#"{"method":"withdraw","id":"p","why":"Decided","call":"toolu_3"}"#);

    let (proposal, form) = proposal();
    let withdrawn = Withdrawal { why: "Decided".into(), call: Some("toolu_3".into()), at_ms: 4 };
    let proposal = Proposal { withdrawn: Some(withdrawn), ..proposal };
    let form = form.replace(
        r#","created_at_ms":1"#,
        r#","withdrawn":{"why":"Decided","call":"toolu_3","at_ms":4},"created_at_ms":1"#,
    );
    keeps_its_form(&proposal, &format!("{{{form}}}"));
}

/// So a client from before a kind of response a newer hub keeps reads the revision as responded to, rather than
/// refusing the proposal, or showing it as waiting.
#[test]
fn a_kind_of_response_from_a_newer_hub_reads_as_unknown() {
    let newer = r#"{"kind":"later","note":"Not yet","at_ms":3}"#;

    assert_eq!(serde_json::from_str::<Response>(newer).unwrap(), Response::Unknown);
}

/// A revision: the ask that writes one, with what changed and an option that keeps its letter, the revision the hub
/// keeps, and the record's summary of each revision, with the start of a note.
#[test]
fn revising_a_proposal_and_its_revisions_in_short_keep_their_form() {
    let mut proposed = proposed();
    proposed.questions[0].options[1].letter = OptionLetter::read("C");
    let written = PROPOSED.replace(r#""body":""}"#, r#""body":"","letter":"C"}"#);
    let revise = Revise {
        id: "p".into(),
        proposed: proposed.clone(),
        changed: "Dropped B".into(),
        call: Some("toolu_2".into()),
        key: "toolu_2".into(),
        images: Vec::new(),
        attachments: Vec::new(),
    };
    keeps_its_form(
        &revise.into_ask(),
        &format!(
            r#"{{"method":"revise","id":"p","proposed":{written},"changed":"Dropped B","call":"toolu_2","key":"toolu_2"}}"#
        ),
    );

    let (first, _) = revision();
    let second = ProposalRevision {
        number: RevisionNumber(2),
        changed: "Dropped B".into(),
        call: Some("toolu_2".into()),
        proposed,
        made_at_ms: 2,
        ..first.clone()
    };
    keeps_its_form(
        &second,
        &format!(r#"{{"number":2,"changed":"Dropped B","call":"toolu_2","proposed":{written},"made_at_ms":2}}"#),
    );

    let chose = ResponseSummary::Chose {
        chosen: vec![ChosenOption {
            header: "Index".into(),
            letter: OptionLetter::read("A").unwrap(),
            label: "Hub".into(),
        }],
        note: "None of".into(),
        at_ms: 3,
    };
    let summaries = [
        RevisionSummary {
            number: RevisionNumber::FIRST,
            call: Some("toolu_1".into()),
            made_at_ms: 1,
            response: Some(chose),
        },
        RevisionSummary { number: RevisionNumber(2), call: None, made_at_ms: 2, response: None },
    ];
    keeps_its_form(
        &summaries,
        r#"[{"number":1,"call":"toolu_1","made_at_ms":1,"response":{"kind":"chose","chosen":[{"header":"Index","letter":"A","label":"Hub"}],"note":"None of","at_ms":3}},{"number":2,"made_at_ms":2}]"#,
    );
}

/// A letter is one from A to Z, so a peer can't name an option by anything else.
#[test]
fn an_options_letter_is_one_from_a_to_z() {
    let read = |letter: &str| serde_json::from_str::<OptionLetter>(&format!("{letter:?}")).ok();

    assert_eq!(read("c"), OptionLetter::read("C"));
    assert_eq!((read("CC"), read("1")), (None, None));
}
