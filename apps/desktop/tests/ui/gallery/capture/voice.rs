//! The steps that have conversation mode ready, and that give its voice a server to fetch its files from, for pictures
//! of how getting it ready stands.

use client_core::conversation::Readiness;
use dictation::fake::FakeEngine;
use fetching::serving::{Reply, serve};

use super::Driver;

/// The voice's weights, its biggest file, which the stalled server sends part of.
const WEIGHTS: &str = "model.q8.gguf";

impl Driver {
    /// Has dictation ready, with a stand-in for its model, and speech on, with the voice's files here, as the gallery's
    /// app has them, so conversation mode starts at once.
    pub(super) fn conversation_ready(&mut self) {
        self.cx.update(|cx| desktop::dictating::use_engine(FakeEngine::new(&[]), cx));
        self.cx.update(|cx| desktop::conversing::set_speech_on(true, cx));
        self.until("dictation and speech to be ready", |d| {
            d.cx.update(|cx| {
                desktop::dictating::is_ready(cx) && desktop::conversing::speech(cx) == Some(Readiness::Ready)
            })
        });
    }

    /// Has the voice's other files here, and its weights from a server that sends enough of them for 40% of the files
    /// and then nothing more until the gallery ends, so getting it ready gets no further.
    pub(super) fn stall_voice_download(&mut self) {
        let files = conversation::FILES;
        let weights = files.files.iter().find(|file| file.path == WEIGHTS).unwrap();
        let others = files.size_bytes() - weights.size_bytes;
        let sent = usize::try_from(files.size_bytes() * 2 / 5 - others).unwrap();
        let claimed = usize::try_from(weights.size_bytes).unwrap();
        let (stalled, _never) = Reply::cut_off(vec![0; sent], claimed).held_after(sent);
        let server = serve(vec![(WEIGHTS.to_string(), vec![stalled])]).unwrap();
        let model_dir = self.dir.join("voice");
        let here = files.dir(&model_dir);
        std::fs::create_dir_all(&here).unwrap();
        for file in files.files.iter().filter(|file| file.path != WEIGHTS) {
            // Sparse, so it takes no room.
            std::fs::File::create(here.join(file.path)).unwrap().set_len(file.size_bytes).unwrap();
        }
        self.cx.update(|cx| desktop::conversing::fetch_the_voice_from(&model_dir, server.base, cx));
        self.until("the voice's files to be 40% fetched", |d| {
            let ready = d.cx.update(|cx| desktop::conversing::speech(cx));
            matches!(ready, Some(Readiness::Downloading(Some(done))) if done > 0.399)
        });
    }

    /// Has the voice's files come from a server that doesn't have them, so getting it ready fails.
    pub(super) fn fail_voice_download(&mut self) {
        let server = serve(Vec::new()).unwrap();
        let model_dir = self.dir.join("voice");
        self.cx.update(|cx| desktop::conversing::fetch_the_voice_from(&model_dir, server.base, cx));
        self.until("getting the voice ready to fail", |d| {
            matches!(d.cx.update(|cx| desktop::conversing::speech(cx)), Some(Readiness::Failed(_)))
        });
    }
}
