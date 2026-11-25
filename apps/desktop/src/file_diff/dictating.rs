//! A comment uses the shared field; final words take the same draft path as typing. Hidden or repurposed comments
//! cannot keep the microphone, even though their input is retained by the diff.

use client_core::hints::Field;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, Context, Entity, Subscription, Window};

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
        let about = move |diff: &Self, cx: &App| {
            let typed = typed.read(cx).text().to_string();
            let review = diff.review.read(cx);
            let listing = review.listing();
            let changed =
                listing.into_iter().flat_map(|listing| listing.rows.iter()).map(|file| file.path.as_ref()).collect();
            let picked = Field::DiffComment { typed: &typed, path: &path, changed }.picked();
            dictation_ui::About::new(diff.dictated_within(cx), picked)
        };
        let on_event =
            |this: &mut Self, event: &dictation_ui::Event, window: &mut Window, cx: &mut Context<Self>| match event {
                dictation_ui::Event::Ended { submit: true } => this.save_draft(window, cx),
                dictation_ui::Event::Ended { submit: false } | dictation_ui::Event::Started => {}
                dictation_ui::Event::Error(message) => cx.emit(FileDiffEvent::Error(message.clone())),
            };
        let place = dictation_ui::Place::DiffComment;
        let (field, attached) = dictation_ui::Field::attach(input, place, enabled, about, on_event, window, cx);
        // Its live words change the comment's height, so the rows are laid out again.
        let laid_out = cx.observe(&field, |this, _, cx| this.lay_out_again(cx));
        (field, attached.into_iter().chain([laid_out]).collect())
    }

    pub(super) fn dictated_within(&self, cx: &App) -> dictation_ui::Within {
        let workspace = Some(self.review.read(cx).workspace_id().to_string());
        dictation_ui::Within { workspace, ..dictation_ui::Within::default() }
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
}
