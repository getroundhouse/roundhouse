//! Reading the web. A search shows what it looked for and how many pages it found, and opens to them, each a link
//! to open in the browser, then what was made of them. One whose agent says only words, as Antigravity's does, says
//! no count, and opens to the words. A fetch shows the site, and opens to the page's address, what was asked of it,
//! and the answer.

use client_core::calls::web::{Link, Results, site};
use client_core::calls::{Fetch, WebSearch};
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement, SharedString, Styled, div,
};
use look::{LineText, Typeset as _};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::find::{self, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

pub(super) fn fetch(fetch: &Fetch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let line = Line::new(IconName::Globe, Fetch::TITLE)
        .part(Part::Chip(look::chip(look::ChipKind::Site, site(fetch.url).to_string())));
    let opened = || Some(fetched(fetch, answer, toggle, cx).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

/// What a fetch opens to: the address, what was asked of the page, if anything was, and what the model made of it.
fn fetched(fetch: &Fetch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    let answer = answer.and_then(Answer::success).filter(|text| !text.trim().is_empty());
    let asked = fetch.prompt.map(|prompt| {
        let marked = find::marked(prompt.to_string().into(), &toggle.marks(Text::Ask, prompt), &[], None);
        div()
            .px(look::Space::Medium)
            .pt(look::Space::XSmall)
            .pb(look::Space::Small)
            .typeset(look::Type::Body)
            .text_color(look::colors(cx).text_muted)
            .child(marked)
    });
    block::frame(cx)
        .child(block::found_label(toggle, Text::Label, fetch.url, cx))
        .children(asked)
        .when_some(answer, |d, answer| d.child(block::separator()).child(block::prose(toggle, Text::Answer, answer)))
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
            gather.written(key, Text::Answer, answer);
        }
    });
}

/// What [`search`] shows open, for finding.
pub(super) fn shown_search(search: &WebSearch, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.plain(key, Text::Pill, search.query);
    line::shown_failure(answer, false, key, gather);
    let Some(results) = results(answer) else { return };
    gather.inside(key, Fold::Output, |gather| {
        for (i, link) in results.links().iter().enumerate() {
            gather.plain(key, Text::nth(Text::Link, i), &link.title);
        }
        if !results.findings().is_empty() {
            gather.written(key, Text::Findings, results.findings());
        }
    });
}

pub(super) fn search(search: &WebSearch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let results = results(answer);
    let line = Line::new(IconName::Globe, WebSearch::TITLE)
        .part(Part::Pill(search.query.to_string().into(), toggle.marks(Text::Pill, search.query)))
        .part(results.as_ref().and_then(Results::summary).map(|summary| Part::Note(summary.into())));
    let opened = || results.map(|results| pages(&results, toggle, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What a search opens to: the pages it found, each a link, then what was made of them.
fn pages(results: &Results, toggle: &Toggle, cx: &App) -> AnyElement {
    let links_area = block::scroll_id(&toggle.key.element("links"));
    let line_text = &toggle.shared.line_text;
    let links: Vec<_> = results
        .links()
        .iter()
        .enumerate()
        .map(|(i, link)| {
            let marks = toggle.marks(Text::nth(Text::Link, i), &link.title);
            link_row(Text::nth(Text::Link, i).element(toggle.key), link, &marks, &links_area, line_text, cx)
        })
        .collect();
    let linked = !links.is_empty();
    let findings = Some(results.findings()).filter(|findings| !findings.is_empty());
    let frame = block::frame(cx).when(linked, |d| {
        let id = toggle.key.element("links");
        d.child(block::scrolling(
            toggle.find,
            &toggle.shared.selection,
            &id,
            div().id(id.clone()).p(look::ROWS_INSET).children(links),
        ))
    });
    frame
        .when_some(findings, |d, findings| {
            d.when(linked, |d| d.child(block::separator())).child(block::prose(toggle, Text::Findings, findings))
        })
        .into_any_element()
}

/// What it found, once it ran.
fn results(answer: Option<&Answer>) -> Option<Results<'_>> {
    let answer = answer?;
    Some(Results::of(answer.success()?, &answer.output))
}

/// A page it found: its title, with its matches, then its site, faint and smaller, on the title's baseline. Clicking
/// it opens the page.
fn link_row(id: ElementId, link: &Link, marks: &Marks, area: &ElementId, line_text: &LineText, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (url, title): (SharedString, SharedString) = (link.url.clone().into(), link.title.clone().into());
    look::list_row(id, look::RowSize::Small)
        .link(title.clone())
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child(line_text.body.set(
            div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(find::marked(
                title.clone(),
                marks,
                &[],
                Some(area.clone()),
            )),
        ))
        .child(line_text.detail.set(div().flex_none().text_color(colors.text_faint).child(site(&link.url).to_string())))
        .into_any_element()
}
