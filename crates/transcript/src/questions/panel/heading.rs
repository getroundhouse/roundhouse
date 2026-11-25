//! The top of the panel: the question's header, a small detail, then its text, at body size.

use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::Typeset as _;
use protocol::capability::asking::Question;

use super::{AroundRequest, QuestionPanel};

/// The height of the header's line, at the kit's extra-small size.
const HEADER_LINE: f32 = 16.;

/// The height of a line of the question, at body size.
const QUESTION_LINE: f32 = 20.;

/// Between the parts of the header.
const BETWEEN: &str = " · ";

/// The question's header, as [`header`] says, then its text. Across from the header is the button that skips what's
/// asked, for what can be skipped.
pub(super) fn heading(
    panel: &QuestionPanel,
    question: Option<&Question>,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let header = header(question, &panel.around);
    let skips = panel.asks.skipped().is_some();
    let skip = skips.then(|| {
        look::icon_button("question-skip", look::ButtonKind::Quiet, look::ControlSize::Small, IconName::Close)
            // Standing out past the line by the room around its icon rather than pushing the question down, so the icon
            // lines up with the edges inside the panel, as the arrows under the question do.
            .stands_out(&[look::Side::Top, look::Side::Right, look::Side::Bottom])
            .tooltip("Skip")
            .on_click(cx.listener(|this, _, _, cx| this.skip(cx)))
    });
    let text = question.map(|question| {
        said("question-text", question.text.clone())
            .w_full()
            .typeset(look::Type::Body)
            .line_height(px(QUESTION_LINE))
            .test_support()
    });
    v_flex()
        .w_full()
        .gap(look::Space::XSmall)
        .child(
            h_flex()
                .w_full()
                .h(px(HEADER_LINE))
                .gap(look::Space::Small)
                .child(
                    said("question-header", header)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .typeset(look::Type::Detail)
                        .text_color(muted)
                        .test_support(),
                )
                .children(skip),
        )
        .children(text)
        .into_any_element()
}

/// The question's own header, then the MCP server the tool asked about is from, then what the subagent that asks is
/// for, where there's each.
fn header(question: Option<&Question>, around: &AroundRequest) -> String {
    let named = question.and_then(|question| question.header.as_deref());
    let parts = [named, around.server.as_deref(), around.asker.as_deref()];
    parts.into_iter().flatten().collect::<Vec<_>>().join(BETWEEN)
}

/// `text` as words the panel says, which assistive apps read as it's drawn, for the caller to style.
pub(super) fn said(id: &'static str, text: impl Into<SharedString>) -> Stateful<Div> {
    let text: SharedString = text.into();
    div().id(id).role(Role::Label).aria_label(text.clone()).child(text)
}

#[cfg(test)]
mod tests {
    use protocol::capability::asking::Question;
    use protocol::fixtures;

    use super::super::AroundRequest;
    use super::header;

    #[test]
    fn the_header_is_the_questions_own_then_the_tools_server_then_the_subagent_that_asks() {
        let question =
            Question { header: Some("Create issue".into()), ..fixtures::question("Use this tool?", &[], false) };
        let of = |server: Option<&str>, asker: Option<&str>| {
            let (server, asker) = (server.map(str::to_string), asker.map(str::to_string));
            header(Some(&question), &AroundRequest { server, asker, more_waiting: 1 })
        };

        let server = Some("GitHub (Northwind)");
        assert_eq!(of(server, Some("Check 1")), "Create issue · GitHub (Northwind) · Check 1");
        assert_eq!(of(server, None), "Create issue · GitHub (Northwind)");
        assert_eq!(of(None, Some("Check 1")), "Create issue · Check 1");
        assert_eq!(of(None, None), "Create issue");
    }
}
