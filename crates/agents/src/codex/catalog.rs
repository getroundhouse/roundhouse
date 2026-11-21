//! The selectable catalog from this app-server, in its own order, across every page.

use std::collections::HashSet;
use std::path::Path;

use protocol::capability::settings::Effort;
use protocol::capability::settings::{EffortOffer, Input, ModelCatalog, ModelOffer, Stage};
use protocol::model::ModelName;
use serde::Deserialize;
use serde_json::json;

use crate::{AgentError, AgentOptions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    data: Vec<NativeModel>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeModel {
    model: ModelName,
    display_name: String,
    description: Option<String>,
    supported_reasoning_efforts: Vec<Reasoning>,
    default_reasoning_effort: Option<Effort>,
    is_default: bool,
    hidden: bool,
    #[serde(default)]
    input_modalities: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reasoning {
    reasoning_effort: Effort,
    display_name: Option<String>,
}

impl NativeModel {
    fn offer(self) -> ModelOffer {
        let efforts: Vec<_> = self
            .supported_reasoning_efforts
            .into_iter()
            .filter(|r| r.reasoning_effort.as_str().is_some())
            .map(|r| match r.display_name {
                Some(label) => EffortOffer::Named { id: r.reasoning_effort, label },
                None => EffortOffer::Id(r.reasoning_effort),
            })
            .collect();
        let default_effort = self.default_reasoning_effort.filter(|e| efforts.iter().any(|offered| offered.id() == e));
        let inputs = self
            .input_modalities
            .iter()
            .filter_map(|s| match s.as_str() {
                "image" => Some(Input::Images),
                "audio" => Some(Input::Audio),
                _ => None,
            })
            .collect();
        ModelOffer {
            model: self.model,
            label: self.display_name,
            description: self.description,
            model_provider: None,
            efforts,
            default_effort,
            speeds: Vec::new(),
            inputs,
            context_tokens: None,
            price: None,
            stage: Stage::Current,
        }
    }
}

pub async fn models_for(options: &AgentOptions, cwd: &Path) -> Result<ModelCatalog, AgentError> {
    let process = super::one_off::open(options, cwd).await?;
    let result = read(&process.shared.rpc).await;
    process.shared.run.stop().await;
    result
}

async fn read(rpc: &super::rpc::Rpc) -> Result<ModelCatalog, AgentError> {
    let mut catalog = ModelCatalog::default();
    let mut seen = HashSet::new();
    let mut cursor = None;
    loop {
        let reply = rpc.ask("model/list", json!({"cursor":cursor,"includeHidden":false})).await?;
        let page: Page = serde_json::from_value(reply)?;
        for model in page.data.into_iter().filter(|m| !m.hidden) {
            if model.is_default {
                catalog.default_model = Some(model.model.clone());
            }
            catalog.offered.push(model.offer());
        }
        let Some(next) = page.next_cursor else { return Ok(catalog) };
        if !seen.insert(next.clone()) {
            return Err(AgentError::Failed("Codex repeated a model/list cursor".into()));
        }
        cursor = Some(next);
    }
}
