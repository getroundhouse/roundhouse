//! The models a kind that offers the same everywhere claims.
use protocol::capability::settings::Effort;
use protocol::capability::settings::{Input, ModelCatalog, ModelOffer, Models, Stage};
use protocol::model::ModelName;

/// `models`, each at every one of `efforts`, starting a chat on the model `starts_on` names, which each thinks at
/// when no effort is chosen.
pub(crate) fn models(models: &[(ModelName, &str)], efforts: &[Effort], starts_on: &(ModelName, Effort)) -> Models {
    let (default_model, default) = starts_on;
    Models::Static(ModelCatalog {
        default_model: Some(default_model.clone()),
        offered: models
            .iter()
            .map(|(model, label)| ModelOffer {
                model: model.clone(),
                label: (*label).into(),
                description: None,
                model_provider: None,
                efforts: efforts.iter().cloned().map(protocol::capability::settings::EffortOffer::Id).collect(),
                default_effort: Some(default.clone()),
                speeds: Vec::new(),
                inputs: vec![Input::Images],
                context_tokens: None,
                price: None,
                stage: Stage::Current,
            })
            .collect(),
    })
}
