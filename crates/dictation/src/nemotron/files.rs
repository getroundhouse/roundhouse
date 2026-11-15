//! The model's files: Fluid Inference's Core ML export of NVIDIA's model, at a revision of its Hugging Face repository
//! pinned so what's downloaded is what was measured. Only the files the engine loads: the 70/2/5 live encoder, the
//! offline encoder, the decoder, the joint that takes one step at a time, and the vocabulary. The model is NVIDIA's,
//! under the OpenMDW-1.1 licence, which lets it be used, and passed on with the licence and its notices. The app is
//! to download them, rather than pass them on.

use crate::model::{ModelFile, ModelFiles};

/// The files the engine loads.
pub const MODEL: ModelFiles = ModelFiles {
    repository: "FluidInference/nemotron-labs-unified-asr-0.6b-coreml",
    revision: "bd13ddde81d3134719c520845346b3252daf3175",
    files: FILES,
};

const FILES: &[ModelFile] = &[
    ModelFile::new(
        "coreml/nemotron_unified_decoder.mlmodelc/analytics/coremldata.bin",
        243,
        "93b62b4a28bf99a52f496dff2b25022d907ded1918936490ed83f0493edbd01b",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_decoder.mlmodelc/coremldata.bin",
        564,
        "9f5ae02afe1022a82c0cddbf3a48ceca947e9e34c33b28b3ce216dbe0b8297ad",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_decoder.mlmodelc/metadata.json",
        3_445,
        "70664a31626b36eda11becaa4b3b378efb7d368405b3eedb075cc4b5fa46f11e",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_decoder.mlmodelc/model.mil",
        13_106,
        "9c89bbd4cc33f55e3a04526e8f17e1dd6952fe2720c65bd61692dd4dec8a0349",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_decoder.mlmodelc/weights/weight.bin",
        18_360_832,
        "ba1e400896aa4ba2e6fa989322401c38948a0c82cbc94d54bc7d2324d4a1acab",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder.mlmodelc/analytics/coremldata.bin",
        243,
        "7b0aa759a1d681e34db4939bdf930b847ee4d5d59fa923c2a4e3509dfc5dde2f",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder.mlmodelc/coremldata.bin",
        538,
        "58c14deacfdd9b6ca6049450c06b6c2f47a0ff1c1627434839e8c41ab837488f",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder.mlmodelc/metadata.json",
        3_293,
        "26e769ef6beea9272ebc2abad4ca81b35d71287556abdc6904891dcfb35a92a1",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder.mlmodelc/model.mil",
        976_715,
        "54152e98e4fb33240243b9aa38eeae46b13052cab20c3a21ff8ef401b16c3cd0",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder.mlmodelc/weights/weight.bin",
        1_195_652_736,
        "c8a477dd6a8e817c66740c5c5b9230be8680e321ddd120239f6ccf965bab31a5",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc/analytics/coremldata.bin",
        243,
        "4128be856dfd2ae6e7ecda898126921e73f76f40c5e68b8ccde1e7d59e87b802",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc/coremldata.bin",
        559,
        "c3b67ed42fa43ff3b95b4e6a3ca70397033b486aff345eb1d49af82e4082d0b1",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc/metadata.json",
        3_328,
        "9323c064defb4c401064c33bd7e79cd882a01b82fc3decf9d2dd957b3030ec66",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc/model.mil",
        793_519,
        "3bda1001a619751f4581ac036c0e5a7f1ef8d91c0ab66435e3879127ec32937e",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc/weights/weight.bin",
        1_184_839_296,
        "21aa74cd4a964268049d99b2d9741d116e7142b1a0a9995b97b6532f8e2b18eb",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_joint_decision_single_step.mlmodelc/analytics/coremldata.bin",
        243,
        "07dc3bb72c56aa7aa0e43173a4f89bf8474f28b110395a4123b266ace6526794",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_joint_decision_single_step.mlmodelc/coremldata.bin",
        560,
        "8148c3f4980957565f5bbe05cff27f20c38f9e79b23c3bac19c0c218901c92ec",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_joint_decision_single_step.mlmodelc/metadata.json",
        3_262,
        "7cfe900ed38b8e564809b0972801741af1e93a267d491cf8a6b0e246b28c38c2",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_joint_decision_single_step.mlmodelc/model.mil",
        9_613,
        "6885efe2bffe960ac1220d89d6dca61e4eb5c09e454fcc8250d8cace3ab13fa5",
    ),
    ModelFile::new(
        "coreml/nemotron_unified_joint_decision_single_step.mlmodelc/weights/weight.bin",
        7_384_000,
        "2b69d8d77dbd0b57f949a2ca705ff49c179a65c241241a98bb16fa5a4e6e6acc",
    ),
    ModelFile::new("coreml/vocab.json", 67_489, "09ae4e49089dd601216ec052dd8bbd474a3709db6e332478ce6a54e47da2ad22"),
];

#[cfg(test)]
mod tests {
    use super::{FILES, MODEL};
    use crate::nemotron::{engine, parts};

    #[test]
    fn everything_the_engine_loads_is_listed() {
        for path in parts::PATHS.into_iter().chain([engine::VOCABULARY]) {
            let listed = FILES.iter().any(|file| file.path == path || file.path.starts_with(&format!("{path}/")));
            assert!(listed, "{path} isn't in FILES");
        }
    }

    #[test]
    fn the_model_is_2_4_gb() {
        assert_eq!(MODEL.size_bytes() / 100_000_000, 24);
    }
}
