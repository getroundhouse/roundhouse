#![cfg(test)]
use protocol::capability::{Control, settings::Settings};
use protocol::event::EventBody;
use protocol::model::ModelName;
use serde_json::{Value, json};

use super::harness::Harness;
use crate::{AgentError, AgentOutput, AgentProcess};

/// What `agy` 1.5.9's `session/new` lists under a Google account's sign-in, Claude's models among them.
fn google_account() -> Value {
    catalog(&["gemini-4-carbon", "gemini-4-hydrogen", "gemini-3.8-flash", "claude-opus-5-5", "claude-sonnet-5-5"])
}

/// What it lists under a Gemini API key, which offers no Claude model.
fn api_key() -> Value {
    catalog(&["gemini-4-carbon", "gemini-4-hydrogen", "gemini-3.8-flash"])
}

fn catalog(models: &[&str]) -> Value {
    let models: Vec<Value> = models.iter().map(|model| json!({ "value": model, "name": model })).collect();
    json!([{
        "on": "session/new",
        "actions": [{
            "result": {
                "sessionId": "session-1",
                "configOptions": [
                    { "id": "model", "currentValue": "gemini-4-carbon", "options": models },
                    {
                        "id": "thought_level",
                        "currentValue": "medium",
                        "options": [
                            { "value": "low", "name": "Low" },
                            { "value": "medium", "name": "Medium" },
                            { "value": "high", "name": "High" }
                        ]
                    }
                ]
            }
        }]
    }])
}

/// The values the adapter set for `config`, in order, as the stand-in recorded them.
fn set(h: &Harness, config: &str) -> Vec<String> {
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    asked
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|request| request["method"] == "session/set_config_option" && request["params"]["configId"] == config)
        .map(|request| request["params"]["value"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_chat_on_claude_opus_runs_it_under_a_google_account() {
    let mut h = Harness::on_model(&google_account(), "claude-opus-5-5");
    let agent = h.started().await;
    let model = loop {
        if let AgentOutput::Event { body: EventBody::AgentInit { model, .. }, .. } = h.next().await {
            break model;
        }
    };
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(set(&h, "model"), ["claude-opus-5-5"]);
    assert_eq!(set(&h, "thought_level"), ["medium"]);
    agent.stop().await;
}

#[tokio::test]
async fn a_chat_on_claude_is_refused_under_a_gemini_api_key_before_agy_is_asked_for_it() {
    let mut h = Harness::on_model(&api_key(), "claude-sonnet-5-5");
    let AgentError::Refused(why) = h.refused().await else { panic!("the launch was refused") };
    assert!(why.contains("Google account"), "{why}");
    assert!(set(&h, "model").is_empty());
}

#[tokio::test]
async fn switching_to_claude_sonnet_needs_a_google_account() {
    let switch =
        Control::Change(Settings { model: Some(ModelName::named("claude-sonnet-5-5")), ..Settings::default() });

    let mut google = Harness::on_model(&google_account(), "gemini-4-carbon");
    let agent = google.started().await;
    agent.control(&switch).await.unwrap();
    assert_eq!(set(&google, "model"), ["gemini-4-carbon", "claude-sonnet-5-5"]);
    agent.stop().await;

    let mut key = Harness::on_model(&api_key(), "gemini-4-carbon");
    let agent = key.started().await;
    let refused = agent.control(&switch).await;
    assert!(matches!(&refused, Err(AgentError::Refused(why)) if why.contains("Google account")), "{refused:?}");
    assert_eq!(set(&key, "model"), ["gemini-4-carbon"]);
    agent.stop().await;
}
