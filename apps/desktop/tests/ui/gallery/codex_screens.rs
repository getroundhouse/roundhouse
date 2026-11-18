use super::frame::{Frame, TABS_MENU};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use client_core::fake_hub::World;

use protocol::model::{AgentKind, Effort, ModelName};

const MODELS: [(&str, &str, Effort, bool); 8] = [
    ("gpt-6.1-sol", "GPT-6.1 Sol", Effort::Low, true),
    ("gpt-6-astra", "GPT-6 Astra", Effort::Medium, true),
    ("gpt-6.1-terra", "GPT-6.1 Terra", Effort::Low, true),
    ("gpt-6-sol", "GPT-6 Sol", Effort::Medium, true),
    ("gpt-6-luna", "GPT-6 Luna", Effort::Medium, false),
    ("gpt-5.6-sol", "GPT-5.6 Sol", Effort::Low, true),
    ("gpt-5.6-terra", "GPT-5.6 Terra", Effort::Medium, true),
    ("gpt-5.6-luna", "GPT-5.6 Luna", Effort::Medium, false),
];

fn models() -> protocol::capability::settings::ModelCatalog {
    use protocol::capability::settings::{Input, ModelCatalog, ModelOffer, Stage};
    ModelCatalog {
        default_model: Some(ModelName::named("gpt-6.1-sol")),
        offered: MODELS
            .into_iter()
            .map(|(model, label, effort, ultra)| ModelOffer {
                model: ModelName::named(model),
                label: label.into(),
                description: None,
                model_provider: Some("OpenAI".into()),
                efforts: Effort::ALL
                    .into_iter()
                    .filter(|level| ultra || *level != Effort::Ultra)
                    .map(protocol::capability::settings::EffortOffer::Id)
                    .collect(),
                default_effort: Some(effort),
                speeds: Vec::new(),
                inputs: vec![Input::Images],
                context_tokens: None,
                price: None,
                stage: Stage::Current,
            })
            .collect(),
    }
}

fn offered() -> World {
    let mut world = super::worlds::titled();
    world.offer_models(AgentKind::Codex, &models());
    world.agents = Some(agents::descriptions());
    world
}

pub(super) const SCREENS: &[Screen] = &[
    Screen {
        name: "new-workspace-agents",
        spec: "workspaces.md",
        world: Some(offered),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1"), Step::Click("new-workspace-agent")],
    },
    Screen {
        name: "codex-models",
        spec: "chats.md",
        world: Some(offered),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Click("new-workspace-agent"),
            Step::ClickItem("Codex"),
            Step::WaitFor("new-workspace-model"),
            Step::Click("new-workspace-model"),
        ],
    },
    Screen {
        name: "antigravity-models",
        spec: "chats.md",
        world: Some(offered),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Click("new-workspace-agent"),
            Step::ClickItem("Antigravity"),
            Step::Click("new-workspace-model"),
        ],
    },
    Screen {
        name: "new-chat-agents",
        spec: "chats.md",
        world: Some(offered),
        frame: TABS_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("new-tab")],
    },
];
