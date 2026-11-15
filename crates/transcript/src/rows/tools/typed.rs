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

/// What a typed call starts with, a monitor's among them, and any line about a monitor.
pub(in crate::rows) const ICON: IconName = IconName::Wrench;

pub(super) fn draw(call: &Call, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let line = Line::new(ICON, typed::title(call)).part(Part::Code(typed::summary(call).into())).shows_output(true);
    // What came back, if anything: a failure that says nothing of what went wrong shows no more than its line does.
    let output = answer.and_then(Answer::text_beside_images).filter(|text| !text.trim().is_empty());
    let opened = || {
        // In the order they're shown, since a call nested in another's row has its texts placed in the order they're
        // made.
        let input = block::code(toggle, Text::Input, "", "", &typed::input(call), cx);
        let output = output.map(|text| block::output(toggle, Text::Output, text, look::colors(cx).text, cx));
        let shown =
            block::frame(cx).child(input).when_some(output, |d, output| d.child(block::separator(cx)).child(output));
        Some(shown.into_any_element())
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
