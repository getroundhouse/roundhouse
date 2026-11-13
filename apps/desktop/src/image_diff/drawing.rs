//! How a changed image is drawn: a file it adds or removes on its own, and one it changes in whichever comparison is
//! chosen, of which side by side is here, and the two in one place are in `stacked`. Both sides are drawn at one scale,
//! from their top left, so a pixel is in the same place in each, and an image that grew looks bigger. Over it are a bar
//! of its path and the choices of what to show, and a strip under that of what's shown: each side, named, with what
//! kind of image it is and how big, and in Difference how much changed.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    SharedString, Styled as _, TestSupportExt as _, WeakEntity, Window, div, px,
};
use look::Hand as _;

use super::{Comparison, ImageDiff, Outcome, Shown, Side, choose, comparison, stacked};
use crate::file_image::{FileImage, Placed, fitted};
use crate::file_view::{HEADER_HEIGHT, placeholder};

/// How tall the strip under the bar is, of smaller text than the bar's.
const STRIP_HEIGHT: Pixels = px(30.);

impl Render for ImageDiff {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Once it's drawn, so a workspace that isn't shown doesn't compare its images.
        self.compare_if_wanted(cx);
        let drawn = self.drawn(cx);
        let body = match &drawn {
            Drawn::Loading => placeholder("Loading…".into(), cx),
            Drawn::Alone(_, image) => alone(image),
            Drawn::Flip(before, after) => stacked::flip(before, after, self.shown, cx.weak_entity()),
            Drawn::Difference(_, _) => stacked::difference(self.difference_shown(), cx),
            Drawn::SideBySide(before, after) => side_by_side(before, after, cx),
            Drawn::Why(why) => placeholder(why.clone(), cx),
        };
        let strip = self.told(&drawn).map(|told| strip(told, cx));
        let choices = self.images().is_some().then(|| choices(self.shown, cx.weak_entity(), cx));
        v_flex().size_full().child(header(self.path.clone(), choices, cx)).children(strip).child(body)
    }
}

/// What's drawn of the change, as its two sides and the comparison chosen say.
enum Drawn<'a> {
    Loading,
    /// A file the change adds or removes, by what it's called, and its one side.
    Alone(&'static str, &'a FileImage),
    Flip(&'a FileImage, &'a FileImage),
    Difference(&'a FileImage, &'a FileImage),
    /// Each side in its own column, with at least one of them an image.
    SideBySide(&'a Side, &'a Side),
    /// Neither side can be drawn, and this says why.
    Why(SharedString),
}

/// What the strip under the bar says of what's shown.
pub(super) struct Told {
    /// Each side shown, by its name, with what kind of image it is and how big, or nothing for one that can't be drawn.
    pub(super) sides: Vec<(&'static str, Option<String>)>,
    /// Whether each side is over its own column, side by side, rather than all of them one after another.
    pub(super) over_columns: bool,
    /// How much changed in its pixels, in Difference, once it's known.
    pub(super) changed: Option<String>,
}

impl ImageDiff {
    fn drawn(&self, cx: &App) -> Drawn<'_> {
        match (&self.before, &self.after) {
            (Side::Loading, _) | (_, Side::Loading) => Drawn::Loading,
            (Side::Absent, Side::Image(image)) => Drawn::Alone("Added", image),
            (Side::Image(image), Side::Absent) => Drawn::Alone("Removed", image),
            (Side::Image(before), Side::Image(after)) => match comparison(cx) {
                Comparison::Flip => Drawn::Flip(before, after),
                Comparison::Difference => Drawn::Difference(before, after),
                Comparison::SideBySide => Drawn::SideBySide(&self.before, &self.after),
            },
            (Side::Image(_), _) | (_, Side::Image(_)) => Drawn::SideBySide(&self.before, &self.after),
            (_, Side::Other(why)) | (Side::Other(why), Side::Absent) => Drawn::Why(why.clone()),
            (Side::Absent, Side::Absent) => Drawn::Why("This file has no changes now".into()),
        }
    }

    /// What the strip under the bar says of what's shown, or `None` while there's no image to say anything of.
    fn told(&self, drawn: &Drawn<'_>) -> Option<Told> {
        let one = |name, image: &FileImage| Told {
            sides: vec![(name, Some(image.describe()))],
            over_columns: false,
            changed: None,
        };
        let about = |side: &Side| match side {
            Side::Image(image) => Some(image.describe()),
            Side::Loading | Side::Absent | Side::Other(_) => None,
        };
        Some(match drawn {
            Drawn::Loading | Drawn::Why(_) => return None,
            Drawn::Alone(name, image) => one(name, image),
            Drawn::Flip(before, after) => match self.shown {
                Shown::Before => one("Before", before),
                Shown::After => one("After", after),
            },
            Drawn::Difference(before, after) => {
                // Both in one line, which has room for what kind they are only when they aren't the same.
                let about = |image: &FileImage| if before.same_kind(after) { image.sizes() } else { image.describe() };
                let changed = match self.difference_shown() {
                    Outcome::Made(difference) => Some(difference.describe()),
                    Outcome::Comparing | Outcome::Failed => None,
                };
                Told {
                    sides: vec![("Before", Some(about(before))), ("After", Some(about(after)))],
                    over_columns: false,
                    changed,
                }
            }
            Drawn::SideBySide(before, after) => Told {
                sides: vec![("Before", about(before)), ("After", about(after))],
                over_columns: true,
                changed: None,
            },
        })
    }

    /// What the strip under the bar says of what's shown, for a test to read.
    #[cfg(feature = "testing")]
    pub(super) fn told_now(&self, cx: &App) -> Option<Told> {
        self.told(&self.drawn(cx))
    }
}

/// The strip under the bar: each side shown, named, and how much changed at its other end. Side by side, each is over
/// its column, the same width, with the same room around and between them, and otherwise they start at the path's
/// start. What's said of each is cut short with an ellipsis when there isn't room.
fn strip(told: Told, cx: &App) -> Div {
    let colors = look::colors(cx);
    let sides = told.sides.into_iter().map(|(name, about)| {
        let about =
            about.map(|about| div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(about));
        h_flex().min_w_0().gap_1p5().child(div().flex_none().text_color(colors.text).child(name)).children(about)
    });
    let strip = h_flex()
        .flex_none()
        .w_full()
        .h(STRIP_HEIGHT)
        .border_b_1()
        .border_color(colors.edge)
        .text_xs()
        .text_color(colors.text_muted);
    if told.over_columns {
        let columns = sides.map(|side| h_flex().flex_1().min_w_0().justify_center().child(side));
        return strip.px_6().gap_6().children(columns);
    }
    // After a swatch of the colour what changed is tinted.
    let changed = told.changed.map(|changed| {
        let swatch = div().flex_none().size(px(8.)).rounded(px(2.)).bg(colors.modified);
        h_flex().flex_none().ml_auto().pl_4().gap_2().child(swatch).child(changed)
    });
    strip.px_4().gap_5().children(sides).children(changed)
}

/// How much faster the folders in its path give up their room than its name, so the name is cut short only once the
/// folders are gone, and the choices stay whole.
const FOLDERS_SHRINK: f32 = 1000.;
/// Room for `…/`, the least of the folders that's shown.
const FOLDERS_LEAST: Pixels = px(16.);

/// Its path, as its folders, muted, and its name, and the choices of what to show, while there are two images.
fn header(path: Option<SharedString>, choices: Option<Div>, cx: &App) -> Div {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let path = path.unwrap_or_default();
    let (within, name) = path.rsplit_once('/').map_or(("", path.as_ref()), |(within, name)| (within, name));
    // Cut from the start, so the folder it's in shows longest, and never to less than `…/`, which a sliver of `…` would
    // read as a dot.
    // Against the name, which room left over once it's cut short would otherwise come between.
    let mut folders = div().overflow_hidden().whitespace_nowrap().text_ellipsis_start().text_right().text_color(muted);
    if !within.is_empty() {
        folders = folders.min_w(FOLDERS_LEAST).child(format!("{within}/"));
    }
    folders.style().flex_shrink = Some(FOLDERS_SHRINK);
    h_flex()
        .flex_none()
        .w_full()
        .h(HEADER_HEIGHT)
        .px_4()
        .border_b_1()
        .border_color(colors.edge)
        .text_sm()
        .child(folders)
        .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name.to_string()))
        .children(choices.map(|choices| choices.ml_auto().pl_4()))
}

/// Side by side, Before, After and Difference, of which Before and After flip one image between the two sides.
fn choices(shown: Shown, view: WeakEntity<ImageDiff>, cx: &App) -> Div {
    let chosen = comparison(cx);
    let flipped = |side: Shown| chosen == Comparison::Flip && shown == side;
    h_flex()
        .flex_none()
        .gap_1()
        .child(choice("image-side-by-side", "Side by side", chosen == Comparison::SideBySide, view.clone(), |cx| {
            choose(Comparison::SideBySide, cx);
        }))
        .child(side("image-before", "Before", Shown::Before, flipped(Shown::Before), view.clone()))
        .child(side("image-after", "After", Shown::After, flipped(Shown::After), view.clone()))
        .child(choice("image-difference", "Difference", chosen == Comparison::Difference, view, |cx| {
            choose(Comparison::Difference, cx);
        }))
}

fn choice(
    id: &'static str,
    label: &'static str,
    selected: bool,
    view: WeakEntity<ImageDiff>,
    chosen: impl Fn(&mut App) + 'static,
) -> Button {
    Button::new(id).hand().ghost().xsmall().selected(selected).label(label).on_click(move |_, _, cx| {
        chosen(cx);
        let _ = view.update(cx, |_, cx| cx.notify());
    })
}

/// One side, shown on its own, which Space and a click on the image flip to the other.
fn side(id: &'static str, label: &'static str, this: Shown, selected: bool, view: WeakEntity<ImageDiff>) -> Button {
    let flipping = view.clone();
    choice(id, label, selected, view, move |cx| {
        choose(Comparison::Flip, cx);
        let _ = flipping.update(cx, |t, cx| t.show_side(this, cx));
    })
    .tooltip("Space flips between them")
}

/// A file the change adds or removes, which has one side, which the strip names.
fn alone(image: &FileImage) -> AnyElement {
    let (shown, covers) = (image.clone(), (image.width, image.height));
    let picture = fitted(covers, Placed::Top, move |scale, cx| shown.whole(scale, cx));
    v_flex().flex_1().min_h_0().w_full().p_6().child(picture.flex_1().w_full()).into_any_element()
}

/// Each side in a column of its own, the same width, so both are drawn at one scale.
fn side_by_side(before: &Side, after: &Side, cx: &App) -> AnyElement {
    let covers = covers([before, after]);
    h_flex()
        .id("images-side-by-side")
        .flex_1()
        .min_h_0()
        .w_full()
        .p_6()
        .gap_6()
        .child(column(before, covers, cx))
        .child(column(after, covers, cx))
        .test_support()
        .into_any_element()
}

/// One side, under where the strip names it, at the top left of what both cover, so a pixel is in the same place in
/// each column.
fn column(side: &Side, covers: (u32, u32), cx: &App) -> Div {
    let body = match side {
        Side::Image(image) => {
            let image = image.clone();
            let picture = fitted(covers, Placed::Top, move |scale, cx| image.within(covers, scale, cx));
            picture.flex_1().w_full().into_any_element()
        }
        Side::Other(why) => placeholder(why.clone(), cx),
        Side::Loading | Side::Absent => placeholder(SharedString::default(), cx),
    };
    v_flex().flex_1().min_w_0().h_full().child(body)
}

/// The pixels the images among `sides` cover between them, from their top left.
fn covers<const N: usize>(sides: [&Side; N]) -> (u32, u32) {
    sides.into_iter().fold((1, 1), |(width, height), side| match side {
        Side::Image(image) => (width.max(image.width), height.max(image.height)),
        Side::Loading | Side::Absent | Side::Other(_) => (width, height),
    })
}
