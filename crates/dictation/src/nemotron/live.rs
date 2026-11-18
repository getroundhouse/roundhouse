//! Hearing a recording live, a window at a time as its audio comes, as `windows` places them.

use super::decoding::{Decoder, Token};
use super::engine::Nemotron;
use super::language::Prompt;
use super::vocab::Vocabulary;
use super::windows::{WINDOW, Window, Windows};
use crate::hints::HintGraph;
use crate::{EngineError, Heard, Hints, Listening, Samples};

pub(super) struct Live<'e> {
    engine: &'e Nemotron,
    /// What the live encoder is told of the recording's language.
    prompt: Prompt,
    hints: HintGraph,
    windows: Windows,
    decoder: Decoder<'e>,
    /// The recording's audio from `kept_from`, which windows to come can still need.
    audio: Vec<f32>,
    kept_from: Samples,
    heard: Samples,
}

impl<'e> Live<'e> {
    pub(super) fn new(engine: &'e Nemotron, hints: &Hints, prompt: Prompt) -> Self {
        let (audio, kept_from, heard) = (Vec::new(), Samples(0), Samples(0));
        let hints = engine.hint_graph(hints);
        Self {
            engine,
            decoder: Decoder::new(engine.left_out.of(&prompt)),
            prompt,
            hints,
            windows: Windows::default(),
            audio,
            kept_from,
            heard,
        }
    }

    /// What's heard in `window`.
    fn run(&mut self, window: Window) -> Result<Vec<Heard>, EngineError> {
        let from = (window.start.audio() - self.kept_from).0;
        let audio = &self.audio[from..from + window.audio().0];
        let mut padded = vec![0.0; WINDOW.0];
        padded[..audio.len()].copy_from_slice(audio);
        let encoded =
            self.engine.encode(&*self.engine.parts.live, &self.prompt, &padded, window.audio(), window.start)?;
        let steps = self.windows.decode(window, encoded.made_out());
        let tokens = self.decoder.decode_live(&*self.engine.parts.transducer, &self.hints, &encoded, steps)?;
        Ok(self.to_heard(tokens))
    }

    fn to_heard(&self, tokens: Vec<Token>) -> Vec<Heard> {
        let vocabulary = &self.engine.vocabulary;
        tokens
            .into_iter()
            .map(|token| (vocabulary.text(token.id), token.at))
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, at)| Heard { text: Vocabulary::spaced(text), at: at.audio() })
            .collect()
    }

    /// Lets go of the audio no window to come needs.
    fn let_go(&mut self) {
        let needed_from = self.windows.needed_from();
        if needed_from > self.kept_from {
            self.audio.drain(..(needed_from - self.kept_from).0);
            self.kept_from = needed_from;
        }
    }
}

impl Listening for Live<'_> {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError> {
        self.audio.extend_from_slice(audio);
        self.heard = self.heard + Samples(audio.len());
        let mut heard = Vec::new();
        while let Some(window) = self.windows.next(self.heard) {
            heard.extend(self.run(window)?);
        }
        self.let_go();
        Ok(heard)
    }

    fn finish(&mut self) -> Result<Vec<Heard>, EngineError> {
        let mut heard = Vec::new();
        while let Some(window) = self.windows.next_at_the_end(self.heard) {
            heard.extend(self.run(window)?);
        }
        let rest = self.decoder.finish_live(&self.hints);
        heard.extend(self.to_heard(rest));
        Ok(heard)
    }

    fn heard_through(&self) -> Samples {
        let decoded = self.windows.decoded_through();
        self.decoder.pending_from().map_or(decoded, |at| at.audio().min(decoded))
    }
}

#[cfg(test)]
mod tests {
    use super::Live;
    use crate::nemotron::engine::Nemotron;
    use crate::nemotron::language::Prompt;
    use crate::nemotron::stand_in;
    use crate::nemotron::vocab::Vocabulary;
    use crate::nemotron::windows::{Steps, WINDOW};
    use crate::{Hints, Language, Listening as _, Samples};

    #[test]
    fn each_step_is_heard_once_in_order_where_it_is_and_audio_no_window_needs_is_let_go_of() {
        let vocabulary = Vocabulary::parse(r#"{"0": "<unk>", "1": "▁a"}"#).unwrap();
        let engine = Nemotron::standing_in(stand_in::never_done_parts(), vocabulary);
        let mut live = Live::new(&engine, &Hints::default(), Prompt::of(Language::Auto));
        let recording = Steps(89);

        let mut heard = Vec::new();
        for buffer in vec![0.0; recording.audio().0].chunks(160) {
            heard.extend(live.hear(buffer).unwrap());
            let kept = Samples(live.audio.len());
            assert!(kept < WINDOW + Steps(2).audio(), "kept {kept:?}");
        }
        heard.extend(live.finish().unwrap());

        let at: Vec<Samples> = heard.iter().map(|heard| heard.at).collect();
        let every_step: Vec<Samples> = (0..recording.0).flat_map(|step| [Steps(step).audio(); 10]).collect();
        assert_eq!(at, every_step);
        assert!(heard.iter().all(|heard| heard.text == " a"));
        assert_eq!(live.heard_through(), recording.audio());
    }
}
