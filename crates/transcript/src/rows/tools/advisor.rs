//! Asking the advisor, a stronger model that reads the whole conversation and advises the agent. Opened, what it
//! said, or, when its reply came encrypted for the model alone, a line saying only the agent can read it.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};

/// What the call opens to when the advisor's reply is encrypted.
const ENCRYPTED: &str = "The advisor's reply is encrypted, so only the agent can read it.";

pub(super) fn draw(answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let encrypted = answer.is_some_and(|answer| answer.encrypted && !answer.is_error);
    let advice = answer.and_then(Answer::success).map(str::trim).filter(|advice| !advice.is_empty());
    // Its start, while it's closed, as a thought's is, so it isn't shown twice.
    let start = advice.filter(|_| !toggle.open).map(Part::start_of);
    let line = Line::new(IconName::Lightbulb, "Ask the advisor").part(start);
    let said = if !toggle.open {
        None
    } else if encrypted {
        let (id, muted) = (toggle.key.element("encrypted"), cx.theme().muted_foreground);
        Some(div().id(id).px_3().py_2().text_sm().text_color(muted).child(ENCRYPTED).test_support().into_any_element())
    } else {
        advice.map(|advice| block::prose(toggle.key, "advice", advice))
    };
    let opened = said.map(|said| block::frame(cx).child(said).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}
