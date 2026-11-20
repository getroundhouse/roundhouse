use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString, Styled as _, Window, div, px,
};

use super::{ChipKind, ChipSize, GithubState, IssueState, PullRequestState};
use crate::baseline::LineText;
use crate::colors::colors;
use crate::counts::lines_changed;
use crate::file_icons::{file_icon_at, folder_icon};
use crate::icon_sizes::IconSizing as _;
use crate::shape::{Corner, EDGE, Shaped as _};
use crate::space::Space;
use crate::typography::Typeset as _;

/// The room at a chip's start and between its parts, a chip's own rather than a space, as the owner chose from
/// pictures: more than the smallest space, which crowded its mark and its name, and less than the next.
const INSIDE: Pixels = px(6.);

/// A chip of `kind` and `size` for what `names` names: a file's or a folder's path, of which it shows the last part, a
/// subagent's words, a server's name, a site's host, a proposal's title, or an issue's or a pull request's number.
pub fn chip(kind: ChipKind, size: ChipSize, names: impl Into<SharedString>) -> Chip {
    Chip { kind, size, names: names.into(), lines: None, changed: None, gone: false, state: None, title: None }
}

/// A thing the app knows, which [`chip`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Chip {
    kind: ChipKind,
    size: ChipSize,
    names: SharedString,
    lines: Option<(u32, Option<u32>)>,
    changed: Option<(usize, usize)>,
    gone: bool,
    state: Option<GithubState>,
    title: Option<SharedString>,
}

/// How wide an issue's or a pull request's title is at most, after its number, before it ends in an ellipsis.
const TITLE_WIDTH: Pixels = px(240.);

impl Chip {
    /// The lines of a file it's about, from `first`, counting from 1, to `last`, or to its end, after its name, such
    /// as `:12–40`, where whoever named the file named them. A regular chip's only, since code's type doesn't come as
    /// small as a small chip's words.
    pub fn lines(mut self, first: u32, last: Option<u32>) -> Self {
        self.lines = Some((first, last));
        self
    }

    /// How many lines were added to a file and removed from it, after its name. A regular chip's only, as its lines
    /// are.
    pub fn changed(mut self, added: usize, removed: usize) -> Self {
        self.changed = Some((added, removed));
        self
    }

    /// Struck through, as a file that was deleted.
    pub fn gone(mut self, gone: bool) -> Self {
        self.gone = gone;
        self
    }

    /// Where an issue or a pull request stands, which its mark shows, in its state's colour: an issue's state for an
    /// issue's chip, and a pull request's for a pull request's. Without it, its mark is muted, as what isn't known yet.
    pub fn state(mut self, state: impl Into<GithubState>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// An issue's or a pull request's title, after its number, muted, and cut short where it's long.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// What it shows of what it names: a path's last part, and the rest whole.
    pub(super) fn name(&self) -> SharedString {
        match self.kind {
            ChipKind::File | ChipKind::Folder => {
                let path = self.names.trim_end_matches('/');
                path.rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or(path).to_string().into()
            }
            ChipKind::Subagent
            | ChipKind::Server
            | ChipKind::Site
            | ChipKind::Proposal
            | ChipKind::Issue
            | ChipKind::PullRequest => self.names.clone(),
        }
    }

    /// The lines it's about, as they're written after its name: `:12`, `:12–40`, or `:12–` to the end.
    pub(super) fn lines_written(&self) -> Option<String> {
        let (first, last) = self.lines?;
        Some(match last {
            Some(last) if last == first => format!(":{first}"),
            Some(last) => format!(":{first}\u{2013}{last}"),
            None => format!(":{first}\u{2013}"),
        })
    }

    /// That it's given only what its size and kind can show: no lines or what changed on a small chip, and no state
    /// of another kind's.
    fn check(&self) {
        debug_assert!(
            self.size == ChipSize::Regular || (self.lines.is_none() && self.changed.is_none()),
            "a small chip has no lines or what changed, since code's type doesn't come as small as its words"
        );
        debug_assert!(
            matches!(
                (self.kind, self.state),
                (_, None)
                    | (ChipKind::Issue, Some(GithubState::Issue(_)))
                    | (ChipKind::PullRequest, Some(GithubState::PullRequest(_)))
            ),
            "{:?}'s chip given {:?}, which is another kind's state",
            self.kind,
            self.state
        );
    }

    /// Its state's mark, in its state's colour, or `unknown`, muted, until its state is known.
    fn state_mark(&self, unknown: IconName, cx: &App) -> AnyElement {
        let (name, color) = match self.state {
            Some(state) => (state.icon(), state.color(cx)),
            None => (unknown, colors(cx).text_muted),
        };
        Icon::new(name).flex_none().sized(self.size.mark()).text_color(color).into_any_element()
    }

    fn mark(&self, cx: &App) -> AnyElement {
        let muted = colors(cx).text_muted;
        let mark = self.size.mark();
        let icon = |name: IconName| Icon::new(name).flex_none().sized(mark).text_color(muted).into_any_element();
        match self.kind {
            ChipKind::File => file_icon_at(&self.names, mark, cx),
            ChipKind::Folder => folder_icon(false, cx).sized(mark).into_any_element(),
            ChipKind::Subagent => icon(IconName::Bot),
            ChipKind::Server => icon(IconName::Plug),
            ChipKind::Site => icon(IconName::Globe),
            ChipKind::Proposal => icon(IconName::Split),
            ChipKind::Issue => self.state_mark(IssueState::Open.icon(), cx),
            ChipKind::PullRequest => self.state_mark(PullRequestState::Open.icon(), cx),
        }
    }
}

impl RenderOnce for Chip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        self.check();
        let (name, lines, mark) = (self.name(), self.lines_written(), self.mark(cx));
        // What's after the name is in code's type, which would ride higher than the name if each were centred, so
        // both go on body text's baseline. Worked out only for a regular chip that has something after its name.
        let regular = self.size == ChipSize::Regular;
        let text = (regular && (lines.is_some() || self.changed.is_some())).then(|| LineText::new(window, cx));
        let name = div()
            .min_w_0()
            .truncate()
            .when(self.gone, |name| name.line_through().text_color(colors.text_muted))
            .child(name);
        let name = match &text {
            Some(text) => text.body.set(name.relative()),
            None => name.typeset(self.size.text()),
        };
        let lines = lines.zip(text.as_ref()).map(|(lines, text)| {
            text.code.set(div().relative().flex_none().text_color(colors.text_muted).child(lines))
        });
        let changed = self
            .changed
            .zip(text.as_ref())
            .map(|((added, removed), text)| lines_changed(added, removed).placed(&text.code));
        let title = self.title.map(|title| {
            div()
                .min_w_0()
                .max_w(TITLE_WIDTH)
                .truncate()
                .typeset(self.size.text())
                .text_color(colors.text_muted)
                .child(title)
        });
        h_flex()
            .flex_none()
            .min_w_0()
            .h(self.size.height())
            .gap(INSIDE)
            .pl(INSIDE)
            .pr(Space::Small)
            .corners(Corner::Control)
            .border(EDGE)
            .border_color(colors.control_edge)
            .bg(colors.raised)
            .text_color(colors.text)
            .whitespace_nowrap()
            .child(mark)
            .child(name)
            .children(lines)
            .children(changed)
            .children(title)
    }
}
