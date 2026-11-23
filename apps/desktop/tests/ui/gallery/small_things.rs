//! The small things that sit in a line, each kind in one picture of its own: every kind of pill, every kind of chip,
//! an issue's and a pull request's in each state and before it's known, with a file's chip with the lines it's about,
//! with what changed in it and deleted, a small chip among words, every kind of badge
//! beside what it's said of, with labels in four colours, and every kind of count beside what it counts, with every
//! kind of lines added and removed, as `look` has them, at the app's scale.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem,
    Render, Styled as _, TestSupportExt as _, Window, div, px, rgb, size,
};
use look::{
    BadgeKind, ChipKind, CountKind, GithubState, IssueState, LinesKind, PillKind, PullRequestState, Type, Typeset as _,
};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// The images' names, in `docs/spec/screens/`.
pub(crate) const PILLS: &str = "pill-kinds";
pub(crate) const CHIPS: &str = "chip-kinds";
pub(crate) const BADGES: &str = "badge-kinds";
pub(crate) const COUNTS: &str = "count-kinds";
/// The spec's file that shows them.
pub(crate) const SPEC: &str = "components.md";
const NAME_WIDTH: f32 = 110.;
const SAMPLE_WIDTH: f32 = 360.;
/// How far the chips' sheet's samples reach past [`SAMPLE_WIDTH`], for a pull request's chip in each state.
const PULL_REQUESTS_PAST: f32 = 60.;
const ROW: f32 = 44.;
const MARGIN: f32 = 16.;

/// A row of a sheet: the name of what it shows, faint, then what it shows, beside the words it's said with.
fn row(name: String, sample: AnyElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    h_flex()
        .h(px(ROW))
        .border_b_1()
        .border_color(colors.edge)
        .child(div().w(px(NAME_WIDTH)).flex_none().typeset(Type::Detail).text_color(colors.text_muted).child(name))
        .child(div().w(px(SAMPLE_WIDTH)).child(sample))
        .into_any_element()
}

/// `words`, then `sample`, in a line, as it's drawn in the app, with the sample's place found by `kind` and `name`.
fn in_a_line(kind: &str, name: &str, words: &'static str, sample: impl IntoElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    h_flex()
        .gap(look::Space::Small)
        .typeset(Type::Body)
        .child(div().flex_none().text_color(colors.text_muted).child(words))
        .child(div().id(places::id(kind, name)).flex().child(sample).test_support())
        .into_any_element()
}

struct Pills;

impl Render for Pills {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let rows: Vec<AnyElement> = PillKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, value) = match kind {
                    PillKind::Plain => ("Ran", "cargo test --workspace"),
                    PillKind::Failure => ("Failed", "error: could not compile `look`"),
                };
                let sample = in_a_line("look::PillKind", &name, words, look::pill(kind, value), cx);
                row(name, sample, cx)
            })
            .collect();
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

struct Chips;

/// A chip of `kind`, an issue's or a pull request's, in each of `states`, each with a number of its own, with its place
/// found by `type_name` and the state's name.
fn in_each_state(kind: ChipKind, type_name: &str, states: impl Iterator<Item = (String, GithubState)>) -> AnyElement {
    let chips = states.enumerate().map(|(number, (name, state))| {
        let chip = look::chip(kind, look::ChipSize::Regular, format!("#{}", 1021 + number)).state(state);
        div().id(places::id(type_name, &name)).flex().child(chip).test_support()
    });
    h_flex().gap(look::Space::XSmall).children(chips).into_any_element()
}

/// An open issue's chip of `size`, with its title, with its place found by the size's name.
fn of_size(size: look::ChipSize) -> AnyElement {
    let [name, ..] = size.written();
    let chip = look::chip(ChipKind::Issue, size, "#1021").state(IssueState::Open).title("Decide how a row says it");
    div().id(places::id(SIZE, &name)).flex().child(chip).test_support().into_any_element()
}

/// What the spec's tables of a chip's sizes and an issue's and a pull request's states name, by which a sheet's place
/// is found.
const SIZE: &str = "look::ChipSize";
const ISSUE_STATE: &str = "look::IssueState";
const PULL_REQUEST_STATE: &str = "look::PullRequestState";

impl Render for Chips {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let mut rows: Vec<AnyElement> = ChipKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, names) = match kind {
                    ChipKind::File => ("Read", "crates/protocol/src/model.rs"),
                    ChipKind::Folder => ("Searched", "crates/protocol/"),
                    ChipKind::Subagent => ("Asked", "Explore"),
                    ChipKind::Server => ("Create issue", "github"),
                    ChipKind::Site => ("Fetched", "docs.rs"),
                    ChipKind::Proposal => ("Proposed", "Holding a train"),
                    ChipKind::Issue => ("Closes", ""),
                    ChipKind::PullRequest => ("", ""),
                };
                let chip = match kind {
                    ChipKind::Issue => in_each_state(
                        kind,
                        ISSUE_STATE,
                        IssueState::ALL.iter().map(|&state| (state.written()[0].clone(), state.into())),
                    ),
                    ChipKind::PullRequest => in_each_state(
                        kind,
                        PULL_REQUEST_STATE,
                        PullRequestState::ALL.iter().map(|&state| (state.written()[0].clone(), state.into())),
                    ),
                    _ => look::chip(kind, look::ChipSize::Regular, names).into_any_element(),
                };
                let sample = in_a_line("look::ChipKind", &name, words, chip, cx);
                row(name, sample, cx)
            })
            .collect();
        let file = || look::chip(ChipKind::File, look::ChipSize::Regular, "crates/protocol/src/model.rs");
        let files = [
            ("Its lines", "Read", file().lines(12, Some(40))),
            ("What changed", "Edit", file().changed(12, 3)),
            ("Deleted", "Delete", look::chip(ChipKind::File, look::ChipSize::Regular, "src/legacy.rs").gone(true)),
        ];
        rows.extend(files.map(|(name, words, chip)| {
            let line = h_flex()
                .gap(look::Space::Small)
                .typeset(Type::Body)
                .child(div().flex_none().text_color(colors.text_muted).child(words))
                .child(chip);
            row(name.to_string(), line.into_any_element(), cx)
        }));
        let unknown = h_flex()
            .gap(look::Space::XSmall)
            .child(look::chip(ChipKind::Issue, look::ChipSize::Regular, "#1021"))
            .child(look::chip(ChipKind::PullRequest, look::ChipSize::Regular, "#1158"));
        rows.push(row("Not known yet".to_string(), unknown.into_any_element(), cx));
        rows.push(row("Regular".to_string(), of_size(look::ChipSize::Regular), cx));
        let among_words = h_flex()
            .gap(look::Space::XSmall)
            .typeset(Type::Body)
            .child("Writing into")
            .child(of_size(look::ChipSize::Small))
            .child("is left.");
        rows.push(row("Small".to_string(), among_words.into_any_element(), cx));
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

/// The labels the sheet draws, in GitHub's colours: a pale one, a bright one, a cool one and a dark one.
const LABELS: [(&str, u32); 4] = [
    ("needs/owner", 0x00fb_ca04),
    ("kind/question", 0x00d8_76e3),
    ("area/sidebar", 0x00c5_def5),
    ("blocked", 0x00b6_0205),
];

struct Badges;

impl Render for Badges {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let mut rows: Vec<AnyElement> = BadgeKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, badge) = match kind {
                    BadgeKind::Plain => ("Work account", "Default"),
                    BadgeKind::Warning => ("Personal account", "Ends in 3 days"),
                    BadgeKind::Failure => ("The request to the model failed", "Twice"),
                };
                let sample = in_a_line("look::BadgeKind", &name, words, look::badge(kind, badge), cx);
                row(name, sample, cx)
            })
            .collect();
        let labels = h_flex()
            .gap(look::Space::XSmall)
            .children(LABELS.map(|(words, color)| look::label(words, Hsla::from(rgb(color)))));
        rows.push(row("Labels".to_string(), labels.into_any_element(), cx));
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

struct Counts;

impl Render for Counts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let line_text = look::LineText::new(window, cx);
        let mut rows: Vec<AnyElement> = CountKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, how_many) = match kind {
                    CountKind::NeedsALook => ("signal-box", 3),
                    CountKind::HowMany => ("Changes", 12),
                };
                let sample = in_a_line("look::CountKind", &name, words, look::count(kind, how_many), cx);
                row(name, sample, cx)
            })
            .collect();
        rows.extend(LinesKind::ALL.iter().map(|&kind| {
            let [name] = kind.written();
            // A workspace's are shortened, and the last of them as last read, rather than kept current.
            let samples: [(usize, usize, &str, bool); 3] = match kind {
                LinesKind::Plain => {
                    [(12, 3, "model.rs", false), (40, 0, "Cargo.toml", false), (0, 120, "legacy.rs", false)]
                }
                LinesKind::Workspace => {
                    [(37_412, 1_250, "alder", false), (48, 0, "birch", false), (12, 3, "cedar", true)]
                }
            };
            let lines = h_flex().gap(look::Space::Large).typeset(Type::Body).text_color(colors.text_muted).children(
                samples.map(|(added, removed, words, as_last_read)| {
                    let lines = look::lines_changed(kind, added, removed).as_last_read(as_last_read);
                    let lines = match kind {
                        LinesKind::Plain => lines,
                        LinesKind::Workspace => lines.written_as(written_counts::shortened),
                    };
                    let lines = lines.placed(&line_text.code);
                    h_flex().gap(look::Space::Small).child(line_text.body.set(div().child(words))).child(lines)
                }),
            );
            let sample = div().id(places::id("look::LinesKind", &name)).flex().child(lines).test_support();
            row(name, sample.into_any_element(), cx)
        }));
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

#[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
fn tall(rows: usize) -> gpui_kit::Pixels {
    px(MARGIN * 2. + rows as f32 * ROW)
}

/// A sheet whose samples reach `past` beyond [`SAMPLE_WIDTH`].
fn wide(past: f32) -> gpui_kit::Pixels {
    px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + past)
}

/// Every kind of pill, with where it draws each.
pub(crate) fn pills(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = PillKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(0.), tall(PillKind::ALL.len()));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Pills,
        |window| Places::of(window, &[("look::PillKind", &names)]),
    )
}

/// Every kind of chip, and a file's with more to say of it, with where it draws each kind.
pub(crate) fn chips(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = ChipKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(PULL_REQUESTS_PAST), tall(ChipKind::ALL.len() + 6));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Chips,
        |window| {
            Places::of(
                window,
                &[
                    ("look::ChipKind", &names),
                    (SIZE, &written(look::ChipSize::ALL.iter().map(|size| size.written()[0].clone()))),
                    (ISSUE_STATE, &written(IssueState::ALL.iter().map(|state| state.written()[0].clone()))),
                    (
                        PULL_REQUEST_STATE,
                        &written(PullRequestState::ALL.iter().map(|state| state.written()[0].clone())),
                    ),
                ],
            )
        },
    )
}

fn written(names: impl Iterator<Item = String>) -> Vec<String> {
    names.collect()
}

/// Every kind of badge, and labels, with where it draws each badge.
pub(crate) fn badges(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = BadgeKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(0.), tall(BadgeKind::ALL.len() + 1));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Badges,
        |window| Places::of(window, &[("look::BadgeKind", &names)]),
    )
}

/// Every kind of count, and every kind of lines added and removed, with where it draws each.
pub(crate) fn counts(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = CountKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let lines: Vec<String> = LinesKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(0.), tall(CountKind::ALL.len() + LinesKind::ALL.len()));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Counts,
        |window| Places::of(window, &[("look::CountKind", &names), ("look::LinesKind", &lines)]),
    )
}
