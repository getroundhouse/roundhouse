//! The model's files: Phonon 31m in English, at a revision of its Hugging Face repository pinned so what's downloaded
//! is what was measured, the one `ptts` 0.7.1 loads by default. Only what the engine loads: its configuration, the
//! tokenizer, the weights quantized to 8 bits, which the CPU runs fastest, and its own voice. The weights and the voice
//! are Gradium's, under Apache-2.0. The app downloads them rather than pass them on.

use models::{ModelFile, ModelFiles};

/// The files the engine loads.
pub const MODEL: ModelFiles = ModelFiles {
    repository: "gradium/phonon-31m-en",
    revision: "92741c5b07d085ff840e26fea8ce43110b87852f",
    files: FILES,
};

const FILES: &[ModelFile] = &[
    ModelFile::new("config.json", 4_634, "b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85"),
    ModelFile::new("tokenizer.json", 277_967, "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936"),
    ModelFile::new("model.q8.gguf", 64_512_352, "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae"),
    ModelFile::new(
        "default-voice.safetensors",
        256_136,
        "dc537478eaadc1a5257fcda2e3501ab81e14c111a5d95a5838f350db2afd0f1a",
    ),
];
