//! What part of the window a screen's picture shows. Most show only what they're about, such as a sidebar or the
//! chat's rows, so a change to one part of the window changes only the pictures of that part. A frame names the parts
//! it's drawn around as [`Part`]s, mostly the [`Region`]s the app marks, and the gallery checks the rest of what it
//! can: that a screen has a dialog open exactly when its frame says so, that nothing open is cut off, and that every
//! part a frame names is drawn.

#![cfg(test)]

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{App, SharedString, Window};
use image::RgbaImage;
use look::Region;

use super::steps::{Screen, Step};
use crate::support::{MENU, first_labelled};

/// How much of the pane shows around what a [`Frame::Around`] or [`Frame::Transcript`] picture is about, in points,
/// so its edges and corners show against the pane.
const ROOM: f32 = 16.;
/// How far past its bounds a menu's edge reaches, in points: the kit draws its ring and shadow outside them, the
/// shadow up to 6 points below it.
const MENU_EDGE: f32 = 8.;
/// How far the chat's rows are inset from the pane's right edge, in points. The strip they leave holds the scrollbar
/// and the outline's ticks, which a [`Frame::Transcript`] takes in only by naming them.
const ROWS_INSET: f32 = 28.;
/// How wide the window is, in points, for a screen about what's in the chat's column. That's room for the composer at
/// its widest, 896 points and 16 of the pane on each side, beside both sidebars at their widest, 480 and 800. So the
/// column is as wide as it gets whatever the sidebars' widths, and a change to one leaves the pictures of the chat
/// alone. The other screens are drawn in the window's usual size, which is how wide the app opens.
pub(crate) const WIDE: f32 = 2208.;

const NOTHING_MORE: &[Part] = &[];
/// The sidebar down the window's left side.
pub(crate) const SIDEBAR: Frame = Frame::Exactly(Part::Region(Region::LeftSidebar), NOTHING_MORE);
/// The sidebar, and a menu opened from it.
pub(crate) const SIDEBAR_MENU: Frame = Frame::Exactly(Part::Region(Region::LeftSidebar), &[Part::Menu]);
/// The sidebar of changes, down the right side.
pub(crate) const CHANGES: Frame = Frame::Exactly(Part::Region(Region::RightSidebar), NOTHING_MORE);
/// The chat's tabs, along the top of the workspace.
pub(crate) const TABS: Frame = Frame::Exactly(Part::Region(Region::TabBar), NOTHING_MORE);
/// The chat's tabs, and a menu opened from them.
pub(crate) const TABS_MENU: Frame = Frame::Exactly(Part::Region(Region::TabBar), &[Part::Menu]);
/// The chat's tabs, and what's drawn in the tab that shows, such as a subagent's work, with the band between.
pub(crate) const TABS_AND_CHAT: Frame = Frame::Transcript(&[Part::Region(Region::TabBar)]);
/// The chat's tabs, and the file the file tab shows.
pub(crate) const FILE: Frame = Frame::Exactly(Part::Region(Region::TabBar), &[Part::Region(Region::OpenFile)]);
/// The composer, and what's written in it.
pub(crate) const COMPOSER: Frame = Frame::Around(Part::Region(Region::Composer), NOTHING_MORE);
/// The composer, and a menu opened from it, such as a picker's.
pub(crate) const COMPOSER_MENU: Frame = Frame::Around(Part::Region(Region::Composer), &[Part::Menu]);
/// The composer, and the prompts queued above it.
pub(crate) const QUEUE: Frame = Frame::Around(Part::Region(Region::Queue), &[Part::Region(Region::Composer)]);
/// The composer, and the card about the chat's context that opens from it.
pub(crate) const CONTEXT: Frame = Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::ContextCard)]);
/// A question the agent asked, which shows in the composer's place.
pub(crate) const QUESTION: Frame = Frame::Around(Part::Region(Region::Question), NOTHING_MORE);
/// The card a finished turn's line opens. It reaches past the chat's column, over the sidebar in a narrow window, and
/// the line has an ID made from the screen's data, so the card is shown by itself.
pub(crate) const TURN_CARD: Frame = Frame::Around(Part::Region(Region::TurnCard), NOTHING_MORE);
/// What's drawn in the chat.
pub(crate) const CHAT: Frame = Frame::Transcript(NOTHING_MORE);
/// What's drawn in the chat, and the outline at the pane's edge, with the list of prompts it opens.
pub(crate) const OUTLINE: Frame = Frame::Transcript(&[Part::Region(Region::Outline)]);

/// Something a frame is drawn around.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Part {
    /// A part of the window the app marks with `look::InRegion::region`, which is what lets a test find it.
    Region(Region),
    /// A line in the chat, by its label, for one whose ID is made from data the screen can't know. Every line can be
    /// found by its label, and one several lines have is the first of them that shows, as a step finds it.
    Line(&'static str),
    /// The menu that's open, with its edge, which the kit draws over whatever's under it, so it's named alongside what
    /// it opened from.
    Menu,
}

/// What a screen's picture shows. A screen shows as little as shows what it's about, so a change to the rest of the
/// window leaves its picture alone. One with something open, such as a menu or what a hover shows, has to show all
/// of it. A frame around parts names at least one, and then any others.
pub(crate) enum Frame {
    /// All of the window, for a screen about how it's laid out, such as with a sidebar hidden. Nothing checks that
    /// it's about that, so the owner judges it when approving the picture.
    Layout,
    /// All of the window, for a screen with a dialog open, which dims all of it. The gallery checks that one is open,
    /// and that every screen with one open has this frame.
    Dialog,
    /// The smallest box around these parts, and nothing more, for parts of the window with edges of their own, such as
    /// a sidebar.
    Exactly(Part, &'static [Part]),
    /// The smallest box around these parts, with [`ROOM`] around it, for something drawn on the pane, such as the
    /// composer. The room is the pane's, so it reaches into a sidebar only beside something that does.
    Around(Part, &'static [Part]),
    /// What's drawn in the chat's rows, with the empty pane around it cut away, and any other parts, such as the
    /// composer, with [`ROOM`] left around it all. The scrollbar and the outline, in the strip along the pane's edge,
    /// are left out unless the outline is named, so the picture is only as wide as the rows are.
    Transcript(&'static [Part]),
}

impl Frame {
    /// Whether everything it shows is in the chat's column, or opened from something there, so its screen is drawn
    /// in a [`WIDE`] window. A frame that names anything as wide as the pane, such as the tab bar, isn't, since a
    /// wider window would only make its picture wider, and the sidebars' widths move it in a window of any width.
    pub(crate) fn in_the_column(&self) -> bool {
        let in_it = |part: &Part| match part {
            Part::Region(region) => region_in_the_column(*region),
            Part::Line(_) | Part::Menu => true,
        };
        match self {
            Frame::Layout | Frame::Dialog | Frame::Exactly(..) => false,
            Frame::Around(first, more) => in_it(first) && more.iter().all(in_it),
            Frame::Transcript(more) => more.iter().all(in_it),
        }
    }
}

/// Whether the region is drawn in the chat's column, or opened from something there. Every region is named, so a new
/// one can't be left out.
fn region_in_the_column(region: Region) -> bool {
    match region {
        Region::Composer
        | Region::Queue
        | Region::Question
        | Region::EditPreview
        | Region::TurnCard
        | Region::ContextCard
        | Region::UsageCard => true,
        Region::LeftSidebar
        | Region::RightSidebar
        | Region::Header
        | Region::TabBar
        | Region::OpenFile
        | Region::SetupLog
        | Region::Transcript
        | Region::Outline
        | Region::Home
        | Region::UpdateDetails => false,
    }
}

/// The part of `image`, a picture of `window`, that the screen's frame shows, using where the window last drew the
/// parts it names. Also what's wrong with the frame, each said as what's wrong with the screen. A frame that can't be
/// worked out shows the whole window, so the rest of what's wrong with it can still be found.
pub(crate) fn crop(screen: &Screen, window: &mut Window, image: &RgbaImage, cx: &mut App) -> (RgbaImage, Vec<String>) {
    let scale = window.scale_factor();
    let whole = Area { left: 0, top: 0, right: image.width(), bottom: image.height() };
    let mut problems = dialog_problem(&screen.frame, window.has_active_dialog(cx)).into_iter().collect::<Vec<_>>();
    let area = match area(&screen.frame, window, image, whole) {
        Ok(area) if area.is_empty() => {
            problems.push("its frame comes to nothing in the window, so it shows the whole window".to_string());
            whole
        }
        Ok(area) => area,
        Err(missing) => {
            problems.push(missing_problem(missing));
            whole
        }
    };
    for open in open(screen) {
        let shown = drawn(window, open, scale).map(|shown| shown.within(whole));
        if shown.is_some_and(|shown| !area.holds(shown)) {
            problems.push(format!("its frame cuts off {open:?}, which is open, so its frame has to name it too"));
        }
    }
    let image = image::imageops::crop_imm(image, area.left, area.top, area.right - area.left, area.bottom - area.top);
    (image.to_image(), problems)
}

/// Whether the part of `image`, a picture of `window`, that the frame shows takes in any of the strip along the
/// chat's right edge, where its scrollbar is drawn. A frame that can't be worked out shows the whole window.
pub(crate) fn shows_the_chats_scrollbar(frame: &Frame, window: &Window, image: &RgbaImage) -> bool {
    let scale = window.scale_factor();
    let whole = Area { left: 0, top: 0, right: image.width(), bottom: image.height() };
    let Some(transcript) = drawn(window, Part::Region(Region::Transcript), scale) else { return false };
    let strip = Area { left: transcript.right.saturating_sub(to_pixels(ROWS_INSET, scale)), ..transcript };
    let shown = area(frame, window, image, whole).unwrap_or(whole);
    !shown.within(strip.within(whole)).is_empty()
}

/// Where in the picture the frame is, or a part it names that the window didn't draw.
fn area(frame: &Frame, window: &Window, image: &RgbaImage, whole: Area) -> Result<Area, Part> {
    let scale = window.scale_factor();
    let room = to_pixels(ROOM, scale);
    let area = match frame {
        Frame::Layout | Frame::Dialog => whole,
        Frame::Exactly(first, more) => covering(window, *first, more, scale)?,
        Frame::Around(first, more) => {
            let covered = covering(window, *first, more, scale)?;
            covered.grown(room).within(covered.and(pane(window, scale, whole)))
        }
        Frame::Transcript(more) => {
            let transcript = covering(window, Part::Region(Region::Transcript), NOTHING_MORE, scale)?.within(whole);
            let rows = Area { right: transcript.right.saturating_sub(to_pixels(ROWS_INSET, scale)), ..transcript };
            let mut covered = drawn_in(image, rows);
            for &part in *more {
                covered = covered.and(drawn(window, part, scale).ok_or(part)?);
            }
            covered.grown(room).within(transcript.and(covered))
        }
    };
    Ok(area.within(whole))
}

/// A dialog dims the whole window, so a picture of only part of it would show part of what it dims.
fn dialog_problem(frame: &Frame, open: bool) -> Option<String> {
    match (frame, open) {
        (Frame::Dialog, false) => Some("its frame is Frame::Dialog, and no dialog is open".to_string()),
        (Frame::Layout | Frame::Exactly(..) | Frame::Around(..) | Frame::Transcript(_), true) => {
            Some("a dialog is open, which dims the whole window, so its frame has to be Frame::Dialog".to_string())
        }
        (Frame::Dialog, true)
        | (Frame::Layout | Frame::Exactly(..) | Frame::Around(..) | Frame::Transcript(_), false) => None,
    }
}

/// What to say of a part the frame names that the window didn't draw, by why that can be.
fn missing_problem(missing: Part) -> String {
    let why = match missing {
        Part::Region(region) => {
            format!("no element marked with `.region(Region::{region:?})` is drawn when its picture is taken")
        }
        Part::Line(label) => format!("no line labelled {label:?} is drawn when its picture is taken"),
        Part::Menu => "no menu is open when its picture is taken".to_string(),
    };
    format!("its frame names {missing:?}, but {why}. `docs/testing.md` says how a screen's frame is chosen")
}

/// What may be open over the window when the picture is taken: a menu, and what each hover waited for, such as a
/// tooltip. Each is checked only if it's drawn. A hover that waits for part of a card is checked only for that part,
/// since nothing the kit opens can be found by itself.
fn open(screen: &Screen) -> impl Iterator<Item = Open> + '_ {
    let shown = screen.steps.iter().filter_map(|step| {
        let Step::Hover { shows, .. } = step else { return None };
        Some(Open::Shown(shows))
    });
    std::iter::once(Open::Part(Part::Menu)).chain(shown)
}

/// Something open over the window, by how it's found.
#[derive(Clone, Copy, Debug)]
enum Open {
    Part(Part),
    /// What a hover waited for, by its ID.
    Shown(&'static str),
}

/// Where the window last drew something open, if it's drawn.
fn drawn(window: &Window, open: impl Into<Open>, scale: f32) -> Option<Area> {
    let open = open.into();
    let found = match open {
        Open::Part(Part::Region(region)) => window.try_find(SharedString::from(region.id())),
        Open::Part(Part::Line(label)) => first_labelled(window, label),
        Open::Part(Part::Menu) => window.try_find(SharedString::from(MENU)),
        Open::Shown(id) => window.try_find(SharedString::from(id)),
    };
    let area = found.filter(ElementSnapshot::visible).map(|found| Area::of(&found, scale))?;
    Some(if matches!(open, Open::Part(Part::Menu)) { area.grown(to_pixels(MENU_EDGE, scale)) } else { area })
}

impl From<Part> for Open {
    fn from(part: Part) -> Self {
        Open::Part(part)
    }
}

/// A box in the picture's pixels, from its left and top edges up to, but not including, its right and bottom ones.
#[derive(Clone, Copy)]
struct Area {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Area {
    fn of(found: &ElementSnapshot, scale: f32) -> Self {
        let bounds = found.bounds();
        let to = |points: gpui_kit::Pixels| to_pixels(f32::from(points), scale);
        Area { left: to(bounds.left()), top: to(bounds.top()), right: to(bounds.right()), bottom: to(bounds.bottom()) }
    }

    fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    fn and(self, other: Area) -> Area {
        Area {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    fn grown(self, by: u32) -> Area {
        Area {
            left: self.left.saturating_sub(by),
            top: self.top.saturating_sub(by),
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    /// Whether all of `other` is inside it.
    fn holds(self, other: Area) -> bool {
        self.left <= other.left && self.top <= other.top && other.right <= self.right && other.bottom <= self.bottom
    }

    /// What of it is inside `other`.
    fn within(self, other: Area) -> Area {
        let (left, top) = (self.left.max(other.left), self.top.max(other.top));
        Area { left, top, right: self.right.min(other.right).max(left), bottom: self.bottom.min(other.bottom).max(top) }
    }
}

/// The smallest box around `first` and every one of `more` as the window last drew them, or one it didn't draw.
fn covering(window: &Window, first: Part, more: &[Part], scale: f32) -> Result<Area, Part> {
    let mut covered = drawn(window, first, scale).ok_or(first)?;
    for &part in more {
        covered = covered.and(drawn(window, part, scale).ok_or(part)?);
    }
    Ok(covered)
}

/// What's between the sidebars, or all of the window where there are none.
fn pane(window: &Window, scale: f32, whole: Area) -> Area {
    let left = drawn(window, Part::Region(Region::LeftSidebar), scale).map_or(whole.left, |sidebar| sidebar.right);
    let right = drawn(window, Part::Region(Region::RightSidebar), scale).map_or(whole.right, |sidebar| sidebar.left);
    Area { left, right, ..whole }
}

/// The smallest box in `area` around everything that isn't the colour of its top left corner, which is the pane's.
/// All of `area` if nothing is drawn there.
fn drawn_in(image: &RgbaImage, area: Area) -> Area {
    if area.is_empty() {
        return area;
    }
    let pane = *image.get_pixel(area.left, area.top);
    let mut drawn: Option<Area> = None;
    for y in area.top..area.bottom {
        for x in area.left..area.right {
            if *image.get_pixel(x, y) != pane {
                let here = Area { left: x, top: y, right: x + 1, bottom: y + 1 };
                drawn = Some(drawn.map_or(here, |drawn| drawn.and(here)));
            }
        }
    }
    drawn.unwrap_or(area)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped at zero first, and a place in a window is far smaller than u32's range"
)]
fn to_pixels(points: f32, scale: f32) -> u32 {
    (points * scale).round().max(0.) as u32
}
