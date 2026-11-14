//! A typed call that has no specialized row yet, with its contract's readable input and result.

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;
use client_core::calls::typed;
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement};
use protocol::capability::calls::Call;

pub(super) fn draw(call: &Call, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let line = Line::new(IconName::Wrench, typed::title(call))
        .part(Part::Code(typed::summary(call).into()))
        .shows_output(true);
    let opened = || {
        Some(
            block::frame(cx)
                .child(block::code(toggle, Text::Input, "", "", &typed::input(call), cx))
                .when_some(answer.and_then(Answer::text_beside_images), |d, text| {
                    d.child(block::separator(cx)).child(block::output(
                        toggle,
                        Text::Output,
                        text,
                        look::colors(cx).text,
                        cx,
                    ))
                })
                .into_any_element(),
        )
    };
    line::drawn(line, answer, opened, toggle, cx)
}

pub(super) fn shown(call: &Call, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.plain(key, Text::Title, &typed::title(call));
    gather.inside(key, Fold::Output, |gather| {
        gather.plain(key, Text::Input, &typed::input(call));
        if let Some(text) = answer.and_then(Answer::text_beside_images) {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}
