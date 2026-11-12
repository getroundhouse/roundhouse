//! Reading the web. A search shows what it looked for and how many pages it found, and opens to them, each a link
//! to open in the browser, then what was made of them. A fetch shows the site, and opens to the page's address, what
//! was asked of it, and the answer.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement as _, Styled, div, px,
};
use look::{Hand as _, LineText};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::calls::web::{Link, Results, site};
use crate::calls::{Fetch, WebSearch};
use crate::find::{self, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

const LINK_HEIGHT: f32 = 24.0;

pub(super) fn fetch(fetch: &Fetch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let line = Line::new(IconName::Globe, "Fetch").part(Part::Chip(site(fetch.url).to_string().into()));
    let opened =
        toggle.open.then(|| {
            let answer = answer.and_then(Answer::success).filter(|text| !text.trim().is_empty());
            block::frame(cx)
                .child(block::found_label(toggle, Text::Label, fetch.url, cx))
                .when_some(fetch.prompt, |d, prompt| {
                    d.child(
                        div().px_3().pt_1().pb_2().text_sm().text_color(cx.theme().muted_foreground).child(
                            find::marked(prompt.to_string().into(), &toggle.marks(Text::Ask, prompt), &[], None),
                        ),
                    )
                })
                .when_some(answer, |d, answer| {
                    d.child(block::separator(cx)).child(block::prose(toggle, Text::Answer, answer))
                })
                .into_any_element()
        });
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`fetch`] shows open, for finding.
pub(super) fn shown_fetch(fetch: &Fetch, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    line::shown_failure(answer, false, key, gather);
    gather.inside(key, Fold::Output, |gather| {
        gather.plain(key, Text::Label, fetch.url);
        if let Some(prompt) = fetch.prompt {
            gather.plain(key, Text::Ask, prompt);
        }
        if let Some(answer) = answer.and_then(Answer::success).filter(|text| !text.trim().is_empty()) {
            gather.markdown(key, Text::Answer, answer);
        }
    });
}

/// What [`search`] shows open, for finding.
pub(super) fn shown_search(search: &WebSearch, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.plain(key, Text::Pill, search.query);
    line::shown_failure(answer, false, key, gather);
    let Some(results) = answer.and_then(Answer::success).map(Results::read) else { return };
    gather.inside(key, Fold::Output, |gather| {
        for (i, link) in results.links.iter().enumerate() {
            gather.plain(key, Text::nth(Text::Link, i), &link.title);
        }
        if !results.summary.is_empty() {
            gather.markdown(key, Text::Findings, results.summary);
        }
    });
}

pub(super) fn search(search: &WebSearch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let results = answer.and_then(Answer::success).map(Results::read);
    let line = Line::new(IconName::Globe, "Search the web")
        .part(Part::Pill(search.query.to_string().into(), toggle.marks(Text::Pill, search.query)))
        .part(results.as_ref().map(|results| Part::Note(results.summary().into())));
    let opened = results.filter(|_| toggle.open).map(|results| {
        let links_area = block::scroll_id(&toggle.key.element("links"));
        let line_text = &toggle.shared.line_text;
        let links = results.links.iter().enumerate().map(|(i, link)| {
            let marks = toggle.marks(Text::nth(Text::Link, i), &link.title);
            link_row(Text::nth(Text::Link, i).element(toggle.key), link, &marks, &links_area, line_text, cx)
        });
        let summary = Some(results.summary).filter(|s| !s.is_empty());
        block::frame(cx)
            .when(!results.links.is_empty(), |d| {
                let id = toggle.key.element("links");
                d.child(block::scrolling(
                    toggle.find,
                    &id,
                    div().id(id.clone()).py_1().children(links.collect::<Vec<_>>()),
                ))
            })
            .when_some(summary, |d, summary| {
                d.when(!results.links.is_empty(), |d| d.child(block::separator(cx))).child(block::prose(
                    toggle,
                    Text::Findings,
                    summary,
                ))
            })
            .into_any_element()
    });
    line::drawn(line, answer, opened, toggle, cx)
}

/// A page it found: its title, with its matches, then its site, faint and smaller, on the title's baseline. Clicking
/// it opens the page.
fn link_row(id: ElementId, link: &Link, marks: &Marks, area: &ElementId, line_text: &LineText, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (url, title): (SharedString, SharedString) = (link.url.clone().into(), link.title.clone().into());
    let hover = theme.muted_foreground.opacity(0.12);
    h_flex()
        .id(id)
        .role(Role::Link)
        .aria_label(title.clone())
        .h(px(LINK_HEIGHT))
        .px_3()
        .gap_2()
        .hand()
        .hover(move |d| d.bg(hover))
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child(line_text.body.set(
            div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(find::marked(
                title.clone(),
                marks,
                &[],
                Some(area.clone()),
            )),
        ))
        .child(line_text.small.set(div().flex_none().text_color(look::faint()).child(site(&link.url).to_string())))
        .into_any_element()
}
