//! One model catalog supplies the capability offers and older clients' model picker.
use protocol::capability::settings::{Input, ModelOffer, Models, Stage};
use protocol::model::{Effort, ModelName};

pub(crate) fn models(models: &[(ModelName, &str)], efforts: &[Effort], default: Effort) -> Models {
    Models {
        offered: models
            .iter()
            .map(|(model, label)| ModelOffer {
                model: model.clone(),
                label: (*label).into(),
                description: None,
                model_provider: None,
                efforts: efforts.iter().copied().map(Into::into).collect(),
                default_effort: Some(default.into()),
                speeds: Vec::new(),
                inputs: vec![Input::Images],
                context_tokens: None,
                price: None,
                stage: Stage::Current,
            })
            .collect(),
    }
}
