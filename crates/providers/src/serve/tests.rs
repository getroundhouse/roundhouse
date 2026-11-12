use std::time::Duration;

use async_trait::async_trait;
use protocol::model::InstanceId;
use protocol::provider::InstanceState;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, WriteHalf};
use tokio::sync::Notify;

use super::*;
use crate::{Capabilities, Feature, Instance, Size, WorkerBinary, WorkerCommand};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Answers everything at once, but for `start`, which waits until it's let go, and `kill`, which panics.
#[derive(Default)]
struct Fake {
    let_start_go: Notify,
}

#[async_trait]
impl Provider for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { targets: vec![], features: vec![Feature::BaseImages] }
    }
    async fn create(
        &self,
        _: &InstanceId,
        _: Option<&InstanceId>,
        _: Option<Size>,
        _: &str,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
    async fn start(&self, _: &InstanceId, _: &[WorkerBinary]) -> Result<(), ProviderError> {
        self.let_start_go.notified().await;
        Ok(())
    }
    async fn stop(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }
    async fn kill(&self, _: &InstanceId) -> Result<(), ProviderError> {
        panic!("a provider with a bug");
    }
    async fn destroy(&self, id: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::NotFound(id.clone()))
    }
    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        Ok(vec![Instance { instance_id: "i".into(), state: InstanceState::Running, base_image: false }])
    }
    async fn connect(&self, _: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        Ok(WorkerCommand { program: "true".into(), args: vec![], env: vec![] })
    }
    async fn make_base_image(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make one in a test"))
    }
}

struct Hub {
    writer: WriteHalf<DuplexStream>,
    replies: Lines<BufReader<tokio::io::ReadHalf<DuplexStream>>>,
}

impl Hub {
    async fn send(&mut self, call: Value) {
        self.writer.write_all(format!("{call}\n").as_bytes()).await.unwrap();
    }

    async fn reply(&mut self) -> Value {
        let line = tokio::time::timeout(TIMEOUT, self.replies.next_line()).await.unwrap().unwrap().unwrap();
        serde_json::from_str(&line).unwrap()
    }

    async fn hello(&mut self, hub_id: &str) -> Value {
        self.send(json!({"id": 0, "request": {"method": "hello", "protocol_version": 1, "oldest_protocol_version": 1,
            "hub_id": hub_id, "build": "test"}}))
            .await;
        self.reply().await
    }
}

/// An adapter serving `provider`, and the hub's end of its stdio.
fn serving(provider: Arc<Fake>) -> Hub {
    let (hub_end, adapter_end) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(adapter_end);
    let made: Arc<dyn Provider> = provider;
    tokio::spawn(async move { serve(Box::new(move |_| Ok(made)), "test", reader, writer).await });
    let (reader, writer) = tokio::io::split(hub_end);
    Hub { writer, replies: BufReader::new(reader).lines() }
}

#[tokio::test]
async fn hello_names_the_adapter_and_what_it_can_do() {
    let mut hub = serving(Arc::default());
    let reply = hub.hello("h").await;
    assert_eq!(
        reply,
        json!({"frame": "reply", "id": 0, "response": {"result": "hello", "protocol_version": 1, "adapter": "fake",
            "build": "test", "capabilities": {"targets": [], "features": ["base_images"]}}})
    );
}

#[tokio::test]
async fn nothing_is_answered_before_hello() {
    let mut hub = serving(Arc::default());
    hub.send(json!({"id": 1, "request": {"method": "list"}})).await;
    assert_eq!(hub.reply().await["error"]["code"], "version_mismatch");
}

#[tokio::test]
async fn a_hub_that_speaks_none_of_the_adapters_versions_is_refused() {
    let mut hub = serving(Arc::default());
    hub.send(json!({"id": 0, "request": {"method": "hello", "protocol_version": 3, "oldest_protocol_version": 2,
        "hub_id": "h", "build": "test"}}))
        .await;
    assert_eq!(hub.reply().await["error"]["code"], "version_mismatch");
}

#[tokio::test]
async fn it_serves_only_the_hub_it_said_hello_to_first() {
    let mut hub = serving(Arc::default());
    hub.hello("h").await;
    assert_eq!(hub.hello("h").await["response"]["result"], "hello", "the same hub can ask again");
    assert_eq!(hub.hello("another").await["error"]["code"], "conflict");
}

#[tokio::test]
async fn answers_carry_the_providers_errors_and_refuse_what_a_newer_hub_asks() {
    let mut hub = serving(Arc::default());
    hub.hello("h").await;
    hub.send(json!({"id": 1, "request": {"method": "destroy", "instance_id": "gone"}})).await;
    assert_eq!(
        hub.reply().await,
        json!({"frame": "reply", "id": 1, "error": {"code": "not_found",
        "message": "instance gone doesn't exist"}})
    );
    hub.send(json!({"id": 2, "request": {"method": "resize", "instance_id": "i"}})).await;
    assert_eq!(hub.reply().await["error"]["code"], "unsupported");
    hub.send(json!({"id": 3, "request": {"method": "make_base_image", "instance_id": "i"}})).await;
    assert_eq!(hub.reply().await["error"]["code"], "unsupported");
}

#[tokio::test]
async fn a_slow_request_doesnt_hold_up_the_rest() {
    let provider = Arc::new(Fake::default());
    let mut hub = serving(provider.clone());
    hub.hello("h").await;
    hub.send(json!({"id": 1, "request": {"method": "start", "instance_id": "i", "workers": []}})).await;
    hub.send(json!({"id": 2, "request": {"method": "list"}})).await;
    let listed = hub.reply().await;
    assert_eq!(listed["id"], 2);
    assert_eq!(listed["response"]["instances"][0]["state"], "running");

    provider.let_start_go.notify_one();
    assert_eq!(hub.reply().await, json!({"frame": "reply", "id": 1, "response": {"result": "ok"}}));
}

#[tokio::test]
async fn it_returns_when_the_hub_goes() {
    let (hub_end, adapter_end) = tokio::io::duplex(1024);
    let (reader, writer) = tokio::io::split(adapter_end);
    let serving = tokio::spawn(async move {
        let made: Arc<dyn Provider> = Arc::new(Fake::default());
        serve(Box::new(move |_| Ok(made)), "test", reader, writer).await
    });
    drop(hub_end);
    tokio::time::timeout(TIMEOUT, serving).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn a_request_whose_answer_panics_is_still_answered() {
    let mut hub = serving(Arc::default());
    hub.hello("h").await;
    hub.send(json!({"id": 1, "request": {"method": "kill", "instance_id": "i"}})).await;
    assert_eq!(hub.reply().await["error"]["code"], "internal");
    hub.send(json!({"id": 2, "request": {"method": "list"}})).await;
    assert_eq!(hub.reply().await["id"], 2, "it goes on serving");
}

#[tokio::test]
async fn a_request_that_doesnt_parse_is_refused_if_it_says_its_id() {
    let mut hub = serving(Arc::default());
    hub.hello("h").await;
    hub.send(json!({"id": 1, "request": {"method": "stop", "instance_id": 3}})).await;
    hub.send(json!({"request": {"method": "stop"}})).await;
    hub.send(json!({"id": 2, "request": {"method": "list"}})).await;
    let refused = hub.reply().await;
    assert_eq!((&refused["id"], &refused["error"]["code"]), (&json!(1), &json!("invalid_request")), "{refused}");
    assert_eq!(hub.reply().await["id"], 2, "one without an ID is passed over");
}
