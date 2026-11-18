//! One model catalog supplies the capability offers and older clients' model picker.
use protocol::capability::settings::{Input, ModelOffer, Models, Stage};
use protocol::model::{Effort, ModelName};

pub(crate) fn models(models: &[(ModelName, &str)], efforts: &[Effort], default: &Effort) -> Models {
    Models::Static {
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
    }
}
