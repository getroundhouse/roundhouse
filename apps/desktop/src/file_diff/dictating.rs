//! A comment uses the shared field; final words take the same draft path as typing. Hidden or repurposed comments
//! cannot keep the microphone, even though their input is retained by the diff.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};

use super::{FileDiff, FileDiffEvent, LineAt};

impl FileDiff {
    pub(super) fn attach_dictation(
        &self,
        input: &Entity<TextareaState>,
        position: &LineAt,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Entity<dictation_ui::Field>, Vec<Subscription>) {
        let enabled = self.dictation_is_shown(position, cx);
        let typed = input.clone();
        let path = position.path.clone();
        let review = self.review.clone();
        let field = cx.new(|cx| {
            let mut field =
                dictation_ui::Field::new(input.clone(), look::Corner::Control, window, cx).with_hints(move |cx| {
                    let mut collector = client_core::hints::HintCollector::new();
                    collector.add_prose(&typed.read(cx).text().to_string());
                    collector.add_path(&path);
                    if let Some(files) = review.read(cx).files.as_ref() {
                        for patch in files {
                            collector.add_path(patch.path());
                        }
                    }
                    dictation_ui::Hints::new(collector.finish())
                });
            field.set_enabled(enabled, cx);
            field
        });
        let events = cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            dictation_ui::Event::Ended { submit: true } => this.save_draft(window, cx),
            dictation_ui::Event::Ended { submit: false } => {}
            dictation_ui::Event::Error(message) => cx.emit(FileDiffEvent::Error(message.clone())),
        });
        let changed = cx.observe(&field, |this, _, cx| this.lay_out_again(cx));
        (field, vec![events, changed])
    }

    pub fn set_dictation_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.dictation_enabled = enabled;
        if let Some(draft) = &self.draft {
            let shown = self.dictation_is_shown(&draft.position, cx);
            draft.dictation.update(cx, |field, cx| field.set_enabled(shown, cx));
        }
    }

    fn dictation_is_shown(&self, position: &LineAt, cx: &App) -> bool {
        self.dictation_enabled
            && self.path.as_ref() == Some(&position.path)
            && self.file(cx).is_some_and(|file| !file.is_binary())
    }

    #[cfg(feature = "testing")]
    pub fn hear(&self, audio: &[f32], cx: &gpui_kit::App) {
        if let Some(draft) = &self.draft {
            draft.dictation.read(cx).hear(audio);
        }
    }
}
