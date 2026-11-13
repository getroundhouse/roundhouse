//! What a prompt is on the wire and in stored rows: what it says beside the request, record or event that carries it,
//! with its files as `attachments`, as it was before a prompt had a type of its own. Each form is read and written as
//! a peer from before reads and writes it.

use protocol::attachment::{Attachment, AttachmentFile, MediaType};
use protocol::client::{ClientFrame, ClientRequest, agent, queue::SessionQueue};
use protocol::event::EventBody;
use protocol::frame;
use protocol::model::{AttachmentId, QueuedPrompt, Seq};
use protocol::prompt::Prompt;
use protocol::worker::{AgentPrompt, WorkerCall, WorkerRequest};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// `form` reads as `value`, and `value` writes as `form`.
fn keeps_its_json_form<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(form: &Value, value: &T) {
    assert_eq!(&serde_json::from_value::<T>(form.clone()).unwrap(), value, "{form} reads as it did");
    assert_eq!(&serde_json::to_value(value).unwrap(), form, "{value:?} writes as it did");
}

fn log() -> Attachment {
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

    // As a client from before attachments, or one that sends no ID, writes it.
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
        &QueuedPrompt { client_msg_id: "m".into(), prompt: Prompt::carrying("why?", vec![log()]) },
    );

    let bare = json!({"client_msg_id": "m", "text": "go"});
    keeps_its_json_form(&bare, &QueuedPrompt { client_msg_id: "m".into(), prompt: Prompt::new("go") });
}

/// Spelled out, since the log holds this form for good.
#[test]
fn a_recorded_prompt_keeps_its_form() {
    let recorded = json!({"type": "user_prompt", "client_msg_id": "m", "text": "", "attachments": [log_form()]});
    let body = EventBody::UserPrompt { client_msg_id: Some("m".into()), prompt: Prompt::carrying("", vec![log()]) };
    keeps_its_json_form(&recorded, &body);

    let bare = json!({"type": "user_prompt", "client_msg_id": null, "text": "hi"});
    keeps_its_json_form(&bare, &EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("hi") });

    // As a hub from before `client_msg_id` recorded it.
    let oldest: EventBody = frame::decode(br#"{"type":"user_prompt","text":"hi"}"#).unwrap();
    assert_eq!(oldest, EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("hi") });
}

#[test]
fn a_prompt_for_the_agent_keeps_its_form() {
    let call = json!({"id": 4, "request": {
        "method": "agent_prompt", "session_id": "s", "prompt": 3, "text": "why?",
        "attachments": [{"attachment": log_form(), "path": "/w/.context/attachments/build.log"}],
    }});
    let file = AttachmentFile { attachment: log(), path: "/w/.context/attachments/build.log".into() };
    let request = WorkerRequest::AgentPrompt(AgentPrompt {
        session_id: "s".into(),
        seq: Seq::new(3),
        prompt: Prompt::carrying("why?", vec![file]),
    });
    keeps_its_json_form(&call, &WorkerCall { id: 4, request });

    let bare = json!({"id": 5, "request": {"method": "agent_prompt", "session_id": "s", "prompt": 3, "text": "go"}});
    let request =
        WorkerRequest::AgentPrompt(AgentPrompt { session_id: "s".into(), seq: Seq::new(3), prompt: Prompt::new("go") });
    keeps_its_json_form(&bare, &WorkerCall { id: 5, request });
}
