//! What a prompt is on the wire and in stored rows: what it says beside the request, record or event that carries it,
//! with its files as `attachments`, as it was before a prompt had a type of its own. Each form is kept as it is, since
//! stored rows hold it and every peer reads and writes it so.

use protocol::attachment::{Attachment, MediaType};
use protocol::client::{ClientFrame, ClientRequest, agent, queue::SessionQueue};
use protocol::event::EventBody;
use protocol::frame;
use protocol::model::{AttachmentId, Place, PromptLine, QueuedPrompt, RevisionNumber, Seq};
use protocol::prompt::{Prompt, PromptFrom};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// `form` reads as `value`, and `value` writes as `form`.
fn keeps_its_json_form<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(form: &Value, value: &T) {
    assert_eq!(&serde_json::from_value::<T>(form.clone()).unwrap(), value, "{form} reads as it did");
    assert_eq!(&serde_json::to_value(value).unwrap(), form, "{value:?} writes as it did");
}

pub(super) fn log() -> Attachment {
    Attachment { id: "a1".into(), name: "build.log".into(), media_type: MediaType::Text, size_bytes: 3 }
}

fn log_form() -> Value {
    json!({"id": "a1", "name": "build.log", "media_type": "text/plain", "size_bytes": 3})
}

#[test]
fn a_prompt_sent_keeps_its_form() {
    let sent = json!({"id": 1, "request": {
        "method": "session_prompt", "id": "s", "client_msg_id": "m", "text": "why?", "attachments": ["a1"],
    }});
    let prompt = Prompt::carrying("why?", vec![AttachmentId::from("a1")]);
    let request =
        ClientRequest::SessionPrompt(agent::SessionPrompt { id: "s".into(), client_msg_id: Some("m".into()), prompt });
    keeps_its_json_form(&sent, &ClientFrame { id: 1, request });

    // One said out loud says so, and one typed leaves it out, as above, so a prompt stored before `spoken` reads as
    // typed.
    let spoken = json!({"id": 1, "request": {
        "method": "session_prompt", "id": "s", "client_msg_id": "m", "text": "is it done?", "spoken": true,
    }});
    let prompt = Prompt::new("is it done?").said_out_loud();
    let request =
        ClientRequest::SessionPrompt(agent::SessionPrompt { id: "s".into(), client_msg_id: Some("m".into()), prompt });
    keeps_its_json_form(&spoken, &ClientFrame { id: 1, request });

    // As a client writes a prompt that carries no file and has no ID.
    let older = br#"{"id":2,"request":{"method":"session_prompt","id":"s","text":"hi","client_msg_id":null}}"#;
    let request = ClientRequest::SessionPrompt(agent::SessionPrompt {
        id: "s".into(),
        client_msg_id: None,
        prompt: Prompt::new("hi"),
    });
    assert_eq!(frame::decode::<ClientFrame>(older).unwrap(), ClientFrame { id: 2, request });
}

#[test]
fn a_prompt_queued_keeps_its_form() {
    let queued = json!({"id": 1, "request": {
        "method": "session_queue", "id": "s", "client_msg_id": "m", "text": "", "attachments": ["a1"],
    }});
    let prompt = Prompt::carrying("", vec![AttachmentId::from("a1")]);
    let request = ClientRequest::SessionQueue(SessionQueue { id: "s".into(), client_msg_id: "m".into(), prompt });
    keeps_its_json_form(&queued, &ClientFrame { id: 1, request });

    let bare = json!({"id": 1, "request": {"method": "session_queue", "id": "s", "client_msg_id": "m", "text": "go"}});
    let prompt = Prompt::new("go");
    let request = ClientRequest::SessionQueue(SessionQueue { id: "s".into(), client_msg_id: "m".into(), prompt });
    keeps_its_json_form(&bare, &ClientFrame { id: 1, request });
}

/// Spelled out, since a session's stored row holds its queue in this form.
#[test]
fn a_queued_prompt_on_its_session_s_record_keeps_its_form() {
    let with_files = json!({"client_msg_id": "m", "text": "why?", "attachments": [log_form()]});
    keeps_its_json_form(
        &with_files,
        &QueuedPrompt {
            client_msg_id: "m".into(),
            prompt: Prompt::carrying("why?", vec![log()]),
            from: PromptFrom::Person,
        },
    );

    let bare = json!({"client_msg_id": "m", "text": "go"});
    keeps_its_json_form(
        &bare,
        &QueuedPrompt { client_msg_id: "m".into(), prompt: Prompt::new("go"), from: PromptFrom::Person },
    );

    let answering = json!({"client_msg_id": "m", "text": "I chose B", "from": {"from": "proposal_response", "proposal": "p", "revision": 1}});
    let from = PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST };
    keeps_its_json_form(
        &answering,
        &QueuedPrompt { client_msg_id: "m".into(), prompt: Prompt::new("I chose B"), from },
    );
}

/// Spelled out, since the log holds this form for good.
#[test]
fn a_recorded_prompt_keeps_its_form() {
    let recorded = json!({"type": "user_prompt", "client_msg_id": "m", "text": "", "attachments": [log_form()]});
    let body = EventBody::UserPrompt {
        client_msg_id: Some("m".into()),
        prompt: Prompt::carrying("", vec![log()]),
        from: PromptFrom::Person,
    };
    keeps_its_json_form(&recorded, &body);

    let bare = json!({"type": "user_prompt", "client_msg_id": null, "text": "hi"});
    keeps_its_json_form(
        &bare,
        &EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("hi"), from: PromptFrom::Person },
    );

    // As the hub records the one it writes to tell the agent the answer to a proposal.
    let answering = json!({
        "type": "user_prompt",
        "client_msg_id": "m",
        "text": "I chose B",
        "from": {"from": "proposal_response", "proposal": "p", "revision": 1},
    });
    let body = EventBody::UserPrompt {
        client_msg_id: Some("m".into()),
        prompt: Prompt::new("I chose B"),
        from: PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST },
    };
    keeps_its_json_form(&answering, &body);

    // As a hub from before `client_msg_id` recorded it.
    let oldest: EventBody = frame::decode(br#"{"type":"user_prompt","text":"hi"}"#).unwrap();
    assert_eq!(
        oldest,
        EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("hi"), from: PromptFrom::Person }
    );
}

/// Spelled out, since a hub lists a chat's prompts as these lines, and says who wrote one only when it wasn't the
/// person, so a line that doesn't say reads as the person's.
#[test]
fn a_prompt_line_says_who_wrote_it_but_for_the_person() {
    let line = |text: &str, from| PromptLine::new(Seq::new(7), text, 0).placed(Place::new(6)).written_by(from);
    let theirs = json!({"seq": 7, "place": 6, "text": "Fix it", "files": 0});
    keeps_its_json_form(&theirs, &line("Fix it", PromptFrom::Person));

    let answer = json!({
        "seq": 7,
        "place": 6,
        "text": "I chose B",
        "files": 0,
        "from": {"from": "proposal_response", "proposal": "p", "revision": 1},
    });
    keeps_its_json_form(
        &answer,
        &line("I chose B", PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST }),
    );
}
