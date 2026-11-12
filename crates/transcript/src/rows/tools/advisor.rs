//! Asking the advisor, a stronger model that reads the whole conversation and advises the agent. Opened, what it
//! said, or a line saying why there's nothing to read: its reply came encrypted for the model alone, hasn't come,
//! never came, or was empty. So an open call always shows something.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};

use super::line::{Line, Part};
use super::{Answer, Reply, Toggle, block, line};
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

/// What the call opens to when the advisor's reply is encrypted.
const ENCRYPTED: &str = "The advisor's reply is encrypted, so only the agent can read it.";
/// What it opens to with no reply so far, whether one is still on its way or its turn ended first. A reply has taken
/// minutes.
const NO_REPLY: &str = "The advisor hasn't replied.";
/// What it opens to when the log says the agent went on without a reply.
const DROPPED: &str = "The advisor didn't reply, and the agent went on without it.";
/// What a call the agent went on without says on its line, so closed, it reads apart from the call the agent then
/// makes again.
const DROPPED_NOTE: &str = "No reply";
/// What it opens to when the reply can be read and holds nothing.
const EMPTY: &str = "The advisor's reply was empty.";

pub(super) fn draw(reply: &Reply, toggle: &Toggle, cx: &App) -> AnyElement {
    let answer = reply.result();
    let advice = advice(answer);
    // Its start, while it's closed, as a thought's is, so it isn't shown twice.
    let start = advice.filter(|_| !toggle.open).map(Part::start_of);
    let dropped = matches!(reply, Reply::Dropped).then(|| Part::Note(DROPPED_NOTE.into()));
    let line = Line::new(IconName::Lightbulb, "Ask the advisor").part(start).part(dropped);
    let said = if toggle.open { said(reply, advice, toggle, cx) } else { None };
    let opened = said.map(|said| block::frame(cx).child(said).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(reply: &Reply, key: &ItemKey, gather: &mut Gather) {
    line::shown_failure(reply.result(), false, key, gather);
    if let Some(advice) = advice(reply.result()) {
        gather.inside(key, Fold::Output, |gather| gather.written(key, Text::Advice, advice));
    }
}

/// What the advisor said, if it can be read and holds something.
fn advice(answer: Option<&Answer>) -> Option<&str> {
    answer.and_then(Answer::success).map(str::trim).filter(|advice| !advice.is_empty())
}

/// What an open call shows: the advice, or why there's none to read. A call that failed shows nothing here, since the
/// line under it says what went wrong.
fn said(reply: &Reply, advice: Option<&str>, toggle: &Toggle, cx: &App) -> Option<AnyElement> {
    if let Some(advice) = advice {
        return Some(block::prose(toggle, Text::Advice, advice));
    }
    let (name, why) = match reply {
        Reply::Nothing => ("no-reply", NO_REPLY),
        Reply::Dropped => ("dropped", DROPPED),
        Reply::Result(answer) if answer.is_error => return None,
        Reply::Result(answer) if answer.encrypted => ("encrypted", ENCRYPTED),
        Reply::Result(_) => ("empty-reply", EMPTY),
    };
    let (id, muted) = (toggle.key.element(name), cx.theme().muted_foreground);
    Some(div().id(id).px_3().py_2().text_sm().text_color(muted).child(why).test_support().into_any_element())
}
