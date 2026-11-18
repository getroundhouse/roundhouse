//! A call to subagents at work: a message to one, a wait for some, or stopping one. Its line names each by what it's
//! for, as the subagent's record says, and has the Open button a subagent's call has where it names one whose work
//! the subagent tab shows. Opened, a message shows what it said, as what the subagent was asked, then what came back.

use client_core::calls::agent::UNTITLED;
use client_core::calls::{ToSubagents, count, first_line};
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled};

use super::agent::{self, Opens};
use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::around::Names;
use crate::rows::shown::Gather;
use crate::rows::written;

pub(super) fn draw(
    to: &ToSubagents,
    answer: Option<&Answer>,
    names: Option<&Names>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let called =
        names.map_or_default(|names| names.subagents.iter().filter_map(|named| named.called.as_deref()).collect());
    let Said { title, called, others } = said(to, called);
    let mut line = Line::new(agent::ICON, title).shows_output(true);
    for called in called {
        line = line.part(Part::Chip(look::chip(look::ChipKind::Subagent, called.to_string())));
    }
    line = line.part(others.map(|others| Part::Note(others.into())));
    let message = message(to);
    if let Some(message) = message {
        line = line.part(Part::Summary(first_line(message).to_string().into()));
    }
    if let Some(open) = open_button(names, toggle) {
        line = line.action(open);
    }
    let output = output(answer);
    let opened = || {
        // In the order they're shown, since a call nested in another's row has its texts placed in the order they're
        // made.
        let asked = message.map(|message| {
            let text = |area| written(toggle, Text::Asked, message, Some(area), None);
            agent::prompt_block(toggle.key, text, toggle.find, &toggle.shared.selection, cx)
        });
        let output = output
            .map(|text| block::frame(cx).child(block::output(toggle, Text::Output, text, look::colors(cx).text, cx)));
        (asked.is_some() || output.is_some()).then(|| {
            v_flex().mb(look::Space::Small).gap(look::Space::Small).children(asked).children(output).into_any_element()
        })
    };
    line::drawn(line, answer, opened, toggle, cx)
}

/// What a call's line says of what it does, and to which subagents.
#[derive(Debug, PartialEq)]
struct Said<'a> {
    title: &'static str,
    /// What each subagent it names is for, or else its kind, as its record says, in its order.
    called: Vec<&'a str>,
    /// How many more it names that no record says anything of, such as `and 1 other`.
    others: Option<String>,
}

/// What the line of `to` says, where `called` is what each subagent it names is for, as those with records say: what
/// it does to them, then each of those, then how many others, or where no record says, only what it does, such as
/// `Wait for subagents`.
fn said<'a>(to: &ToSubagents, called: Vec<&'a str>) -> Said<'a> {
    if called.is_empty() {
        return Said { title: to.title(), called, others: None };
    }
    let unnamed = to.subagents().len().saturating_sub(called.len());
    let others = (unnamed > 0).then(|| format!("and {}", count(unnamed, "other", "others")));
    Said { title: to.verb(), called, others }
}

/// What [`draw`] shows open, for finding: what a message said, and what came back.
pub(super) fn shown(to: &ToSubagents, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.inside(key, Fold::Output, |gather| {
        if let Some(message) = message(to) {
            gather.written(key, Text::Asked, message);
        }
        if let Some(text) = output(answer) {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}

/// What a message said, which the subagent takes as what it's asked.
fn message<'a>(to: &ToSubagents<'a>) -> Option<&'a str> {
    if let ToSubagents::Message { message, .. } = to { Some(message) } else { None }
}

/// What came back, if it says anything: a failure that says nothing of what went wrong shows no more than its line.
fn output(answer: Option<&Answer>) -> Option<&str> {
    answer.and_then(Answer::text_beside_images).filter(|text| !text.trim().is_empty())
}

/// The button that opens the subagent tab on the subagent the call names, where there's one, as [`Names::opened`]
/// says.
fn open_button(names: Option<&Names>, toggle: &Toggle) -> Option<AnyElement> {
    let subagent = names?.opened()?;
    let id = toggle.key.element("open-subagent");
    let title = subagent.called.as_deref().unwrap_or(UNTITLED);
    Some(agent::open_tab(id, Opens::Subagent(subagent.id.clone()), title, &toggle.shared.this))
}

#[cfg(test)]
mod tests {
    use client_core::calls::ToSubagents;
    use protocol::model::SubagentId;

    use super::{Said, said};

    /// A call's line names each subagent a record says what it's for, after what it does to them, and says how many
    /// others it names. Where no record says, as for an agent that keeps none, the line says only what it does.
    #[test]
    fn a_call_s_line_names_the_subagents_its_records_say_what_they_re_for() {
        let ids: Vec<SubagentId> = ["thread-1", "thread-2", "conv-7"].map(Into::into).to_vec();
        let wait = ToSubagents::Wait { subagents: &ids };
        let stop = ToSubagents::Stop { subagent: &ids[0] };

        let others = Some("and 2 others".to_string());
        assert_eq!(
            said(&wait, vec!["/root/reviewer"]),
            Said { title: "Wait for", called: vec!["/root/reviewer"], others }
        );
        assert_eq!(said(&stop, vec!["/root/reviewer"]).others, None, "naming all it names");
        assert_eq!(said(&stop, Vec::new()), Said { title: "Stop a subagent", called: Vec::new(), others: None });
    }
}
