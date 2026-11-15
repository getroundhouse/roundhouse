//! The steps that dictate into the composer, to a stand-in for the model, for a picture of what's heard as it's said,
//! and that give dictation a made-up model, for pictures of how it stands.

use std::path::Path;

use desktop::workspace_pane::LIVE_LINE;
use dictation::fake::FakeEngine;
use dictation::model::{ModelFile, ModelFiles};
use fetching::serving::{Reply, serve};
use gpui_kit::test::TestWindowExt as _;

use super::Driver;
use crate::support::made_up_model::{made_up, put};

/// Half of a made-up model of 2.4 GB, as big as it says and empty, which only its size says is there. Only a download
/// that finishes checks its hash, which none does here.
const HALF: ModelFile = ModelFile { path: "coreml/vocab.json", size_bytes: 1_200_000_000, sha256: [0; 32] };
const OTHER_HALF: ModelFile =
    ModelFile { path: "coreml/encoder.mlmodelc/weights/weight.bin", size_bytes: 1_200_000_000, sha256: [0; 32] };
const MODEL: ModelFiles = ModelFiles { repository: "test", revision: "made-up", files: &[HALF, OTHER_HALF] };

impl Driver {
    /// Dictates `said` from the microphone button, and waits for the line under what's typed to show `shows`.
    pub(super) fn dictate(&mut self, said: &str, shows: &str) {
        let words: Vec<&str> = said.split(' ').collect();
        let engine = FakeEngine::new(&words);
        let audio = engine.saying(said);
        self.cx.update(|cx| desktop::dictating::use_engine(engine, cx));
        self.until("dictation to be ready", |d| d.cx.update(|cx| desktop::dictating::is_ready(cx)));
        self.click("dictate");
        self.update_pane(|pane, _| pane.hear(&audio));
        self.until(&format!("the live text to be {shows:?}"), |d| {
            let live =
                |window: &gpui_kit::Window| window.try_find(LIVE_LINE).and_then(|l| l.label().map(str::to_string));
            d.cx.update_window(d.handle, |_, window, _| live(window)).unwrap().as_deref() == Some(shows)
        });
    }

    /// Gives dictation half of a made-up model, and the rest from a server that sends nothing of it, so turning
    /// dictation on shows a download under way that gets no further.
    pub(super) fn stall_dictation_download(&mut self) {
        // Says the file is all there, and sends none of it until the gallery ends.
        let (stalled, _never) = Reply::cut_off(Vec::new(), 1_200_000_000).held_after(0);
        let server = serve(vec![(OTHER_HALF.path.to_string(), vec![stalled])]).unwrap();
        let models = self.dir.join("models");
        let here = MODEL.dir(&models).join(HALF.path);
        std::fs::create_dir_all(here.parent().unwrap()).unwrap();
        // Sparse, so it takes no room.
        std::fs::File::create(here).unwrap().set_len(HALF.size_bytes).unwrap();
        let load = |_: &Path| Ok(FakeEngine::new(&[]));
        self.cx.update(|cx| desktop::dictating::use_model(models, MODEL, server.base, load, cx));
    }

    /// Gives dictation a made-up model that's here, as big as the real one says it is, so turning dictation on has it
    /// ready at once.
    pub(super) fn dictation_model_here(&mut self) {
        let models = self.dir.join("models");
        for file in MODEL.files {
            let at = MODEL.dir(&models).join(file.path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::File::create(at).unwrap().set_len(file.size_bytes).unwrap();
        }
        let load = |_: &Path| Ok(FakeEngine::new(&[]));
        self.cx.update(|cx| desktop::dictating::use_model(models, MODEL, String::new(), load, cx));
    }

    /// Gives dictation a made-up model from a server that doesn't have it, so turning dictation on fails.
    pub(super) fn fail_dictation_download(&mut self) {
        let (files, bodies) = made_up(&["coreml/vocab.json", "coreml/encoder.mlmodelc/weights/weight.bin"], 1000);
        let models = self.dir.join("models");
        put(&models, &files, &files.files[0], &bodies[0]);
        let server = serve(Vec::new()).unwrap();
        let load = |_: &Path| Ok(FakeEngine::new(&[]));
        self.cx.update(|cx| desktop::dictating::use_model(models, files, server.base, load, cx));
    }
}
