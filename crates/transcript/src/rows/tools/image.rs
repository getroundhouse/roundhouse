//! Making an image: what it was asked for, and opened, all of what it was asked for, then the image it made. What it
//! was asked for is what came back with the image, as the model rewrote it, or else what the call asked, since Codex
//! says it only once the image is made.

use client_core::calls::first_line;
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, div};
use look::Typeset as _;
use protocol::capability::calls::CallOutput;
use protocol::capability::calls::call::GenerateImage;

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::find::{self, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

const TITLE: &str = "Make an image";

pub(super) fn draw(made: &GenerateImage, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let asked = asked(made, answer);
    let summary = asked.map(|asked| Part::Summary(first_line(asked).to_string().into()));
    let line = Line::new(IconName::ImagePlus, TITLE).part(summary);
    let images = answer.map_or_default(Answer::images);
    let opened = || {
        if asked.is_none() && images.is_empty() {
            return None;
        }
        let said = asked.map(|asked| {
            let marked = find::marked(asked.to_string().into(), &toggle.marks(Text::Ask, asked), &[], None);
            div()
                .px(look::Space::Medium)
                .py(look::Space::Small)
                .typeset(look::Type::Body)
                .text_color(look::colors(cx).text_muted)
                .child(marked)
        });
        let shown = block::frame(cx)
            .children(said)
            .when(asked.is_some() && !images.is_empty(), |d| d.child(block::separator(cx)))
            .when(!images.is_empty(), |d| d.child(block::images(images, cx)));
        Some(shown.into_any_element())
    };
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(made: &GenerateImage, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    line::shown_failure(answer, false, key, gather);
    if let Some(asked) = asked(made, answer) {
        gather.inside(key, Fold::Output, |gather| gather.plain(key, Text::Ask, asked));
    }
}

/// What it was asked to make, if anything says: what came back with the image, or else the call.
fn asked<'a>(made: &'a GenerateImage, answer: Option<&'a Answer>) -> Option<&'a str> {
    let returned = answer.and_then(|answer| {
        let CallOutput::Image { prompt, .. } = &*answer.output else { return None };
        prompt.as_deref()
    });
    returned.or(Some(made.prompt.as_str())).filter(|asked| !asked.trim().is_empty())
}
