use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString, Styled as _, Window, div, px,
};

use super::ChipKind;
use crate::colors::colors;
use crate::counts::lines_changed;
use crate::file_icons::{file_icon_at, folder_icon};
use crate::icon_sizes::{IconSize, IconSizing as _};
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How tall a chip is: as tall as a pill, so the two sit in one line.
const HEIGHT: Pixels = px(20.);
/// Its mark's size, a regular icon, since it sits beside body text.
const MARK: IconSize = IconSize::Regular;

/// A chip of `kind` for what `names` names: a file's or a folder's path, of which it shows the last part, a subagent's
/// words, a server's name or a site's host.
pub fn chip(kind: ChipKind, names: impl Into<SharedString>) -> Chip {
    Chip { kind, names: names.into(), lines: None, changed: None, gone: false }
}

/// A thing the app knows, which [`chip`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Chip {
    kind: ChipKind,
    names: SharedString,
    lines: Option<(u32, Option<u32>)>,
    changed: Option<(usize, usize)>,
    gone: bool,
}

impl Chip {
    /// The lines of a file it's about, from `first`, counting from 1, to `last`, or to its end, after its name, such
    /// as `:12–40`, where whoever named the file named them.
    pub fn lines(mut self, first: u32, last: Option<u32>) -> Self {
        self.lines = Some((first, last));
        self
    }

    /// How many lines were added to a file and removed from it, after its name.
    pub fn changed(mut self, added: usize, removed: usize) -> Self {
        self.changed = Some((added, removed));
        self
    }

    /// Struck through, as a file that was deleted.
    pub fn gone(mut self, gone: bool) -> Self {
        self.gone = gone;
        self
    }

    /// What it shows of what it names: a path's last part, and the rest whole.
    pub(super) fn name(&self) -> SharedString {
        match self.kind {
            ChipKind::File | ChipKind::Folder => {
                let path = self.names.trim_end_matches('/');
                path.rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or(path).to_string().into()
            }
            ChipKind::Subagent | ChipKind::Server | ChipKind::Site => self.names.clone(),
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

    fn mark(&self, cx: &App) -> AnyElement {
        let muted = colors(cx).text_muted;
        let icon = |name: IconName| Icon::new(name).flex_none().sized(MARK).text_color(muted).into_any_element();
        match self.kind {
            ChipKind::File => file_icon_at(&self.names, MARK, cx),
            ChipKind::Folder => folder_icon(false, cx).sized(MARK).into_any_element(),
            ChipKind::Subagent => icon(IconName::Bot),
            ChipKind::Server => icon(IconName::Plug),
            ChipKind::Site => icon(IconName::Globe),
        }
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let (name, lines, mark) = (self.name(), self.lines_written(), self.mark(cx));
        let name = div()
            .min_w_0()
            .truncate()
            .when(self.gone, |name| name.line_through().text_color(colors.text_muted))
            .child(name);
        h_flex()
            .flex_none()
            .min_w_0()
            .h(HEIGHT)
            .gap(Space::XSmall)
            .pl(Space::XSmall)
            .pr(Space::Small)
            .corners(Corner::Control)
            .border_1()
            .border_color(colors.control_edge)
            .bg(colors.raised)
            .typeset(Type::Body)
            .text_color(colors.text)
            .whitespace_nowrap()
            .child(mark)
            .child(name)
            .when_some(lines, |chip, lines| {
                chip.child(div().flex_none().typeset(Type::Code).text_color(colors.text_muted).child(lines))
            })
            .when_some(self.changed, |chip, (added, removed)| chip.child(lines_changed(added, removed)))
    }
}
