use crate::file_image::FileImage;
use gpui_kit::{Hsla, RenderImage};
use image::{Frame, Rgba, RgbaImage};
use std::cell::RefCell;
use std::sync::Arc;

/// How far apart, in pixels, changes can be and still be one place, by the boxes around them, so a word's letters are
/// one place rather than one for each letter.
pub(super) const CELL: u32 = 16;
/// How wide the squares are that changes are first gathered in, a place being the squares with a change in them that
/// touch one another. Changes in two squares that touch are less than two squares, [`CELL`], apart, so gathering them
/// never makes one place of changes that joining their boxes wouldn't.
const SQUARE: u32 = CELL / 2;
/// How much of its brightness each pixel that hasn't changed keeps, in grey, and how bright its darkest is, so a dark
/// screenshot can still be read around the changes, and what changed is the one colour there.
const FADED: f32 = 0.6;
const DARKEST: f32 = 16.;
/// How much of a pixel that changed is the colour of what changed, with the rest its own brightness, in grey, so what's
/// there still shows through.
const TINT: f32 = 0.6;

/// What changed between two images.
pub(crate) struct Difference {
    /// The picture of it: the image after, faded, with what changed tinted, in GPUI's order of channels.
    pub(crate) picture: Arc<RenderImage>,
    /// Its pixels, which cover both images.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// How many of its pixels changed.
    pub(crate) changed: u64,
    /// Where they are, in order down and then across.
    pub(crate) places: Vec<Place>,
    /// What only one of the two covers, which isn't compared.
    pub(crate) uncovered: Vec<Place>,
    /// The places last drawn, with those whose outlines would come too near each other at the scale they were drawn
    /// at made one, by how far apart that made them, which stays the same until the room it's drawn in changes.
    pub(super) drawn: RefCell<Option<(u32, Arc<[Place]>)>>,
}

/// A box around one place that changed, in the picture's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Difference {
    /// Compares the two images, which takes a while for big ones, so it's for a background thread. `colour` is what a
    /// pixel that changed is tinted. `None` if either's pixels can't be had.
    pub(crate) fn between(before: &FileImage, after: &FileImage, colour: Rgba<u8>) -> Option<Difference> {
        let (before, after) = (before.pixels()?, after.pixels()?);
        let changes = Changes::between(&before, &after);
        let sizes = [before.dimensions(), after.dimensions()];
        let mut picture = changes.picture(&before, &after, colour);
        // GPUI draws what it's given as blue, green, red and alpha.
        for pixel in picture.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some(Difference {
            picture: Arc::new(RenderImage::new(vec![Frame::new(picture)])),
            width: changes.width,
            height: changes.height,
            changed: changes.count(),
            places: changes.places(),
            uncovered: uncovered(sizes),
            drawn: RefCell::new(None),
        })
    }

    /// The places, with any two of them that come within `apart` pixels of each other made one, which is how far apart
    /// they need to be for their outlines not to meet at the scale they're drawn at. Those at their own size, already
    /// as far apart as [`CELL`], are the places as they're counted.
    pub(crate) fn places_drawn(&self, apart: u32) -> Arc<[Place]> {
        let mut drawn = self.drawn.borrow_mut();
        if let Some((at, places)) = drawn.as_ref()
            && *at == apart
        {
            return places.clone();
        }
        let places: Arc<[Place]> =
            if apart <= CELL { self.places.clone().into() } else { joined(self.places.clone(), apart).into() };
        *drawn = Some((apart, places.clone()));
        places
    }
}

/// What only one of images of `sizes` covers, laid from their top left: of each, what's to the right of what both
/// cover, all the way down it, and what's below, as wide as both cover.
pub(super) fn uncovered(sizes: [(u32, u32); 2]) -> Vec<Place> {
    let shared = (sizes[0].0.min(sizes[1].0), sizes[0].1.min(sizes[1].1));
    let mut uncovered = Vec::new();
    for (width, height) in sizes {
        if width > shared.0 {
            uncovered.push(Place { x: shared.0, y: 0, width: width - shared.0, height });
        }
        if height > shared.1 {
            uncovered.push(Place { x: 0, y: shared.1, width: shared.0, height: height - shared.1 });
        }
    }
    uncovered
}

/// Which pixels differ, of what both images cover, over what either does.
pub(super) struct Changes {
    pub(super) width: u32,
    pub(super) height: u32,
    /// One for each pixel, row by row.
    pub(super) changed: Vec<bool>,
}

impl Changes {
    pub(super) fn between(before: &RgbaImage, after: &RgbaImage) -> Changes {
        let (width, height) = (before.width().max(after.width()), before.height().max(after.height()));
        let shared = (before.width().min(after.width()), before.height().min(after.height()));
        let mut changed = Vec::with_capacity(usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(0));
        for y in 0..height {
            for x in 0..width {
                let both = x < shared.0 && y < shared.1;
                changed.push(both && before.get_pixel(x, y) != after.get_pixel(x, y));
            }
        }
        Changes { width, height, changed }
    }

    pub(super) fn count(&self) -> u64 {
        self.changed.iter().filter(|changed| **changed).count().try_into().unwrap_or(u64::MAX)
    }

    pub(super) fn at(&self, x: u32, y: u32) -> bool {
        let at = u64::from(y) * u64::from(self.width) + u64::from(x);
        usize::try_from(at).ok().and_then(|at| self.changed.get(at).copied()).unwrap_or(false)
    }

    /// The image after, or before where only that covers it, faded to grey, with what changed tinted `colour`.
    pub(super) fn picture(&self, before: &RgbaImage, after: &RgbaImage, colour: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_fn(self.width, self.height, |x, y| {
            let Some(Rgba([red, green, blue, alpha])) =
                after.get_pixel_checked(x, y).or(before.get_pixel_checked(x, y))
            else {
                return Rgba([0, 0, 0, 0]);
            };
            let brightness = 0.299 * f32::from(*red) + 0.587 * f32::from(*green) + 0.114 * f32::from(*blue);
            if self.at(x, y) {
                let tinted = |of: u8| channel(TINT * f32::from(of) + (1. - TINT) * brightness);
                return Rgba([tinted(colour[0]), tinted(colour[1]), tinted(colour[2]), u8::MAX]);
            }
            let grey = channel(DARKEST + FADED * brightness);
            Rgba([grey, grey, grey, *alpha])
        })
    }

    /// The places that changed: each group of squares [`SQUARE`] pixels wide with a change in them, touching one
    /// another at a side or a corner, as the box around them, with places whose boxes come within [`CELL`] of each
    /// other made one, so no box is drawn inside, across or against another.
    pub(super) fn places(&self) -> Vec<Place> {
        let (across, down) = (self.width.div_ceil(SQUARE), self.height.div_ceil(SQUARE));
        let cells = |x: u32, y: u32| usize::try_from(u64::from(y) * u64::from(across) + u64::from(x)).unwrap_or(0);
        let mut touched = vec![false; cells(0, down)];
        for y in 0..self.height {
            for x in (0..self.width).filter(|x| self.at(*x, y)) {
                touched[cells(x / SQUARE, y / SQUARE)] = true;
            }
        }
        let mut seen = vec![false; touched.len()];
        let mut places = Vec::new();
        for start in (0..down).flat_map(|y| (0..across).map(move |x| (x, y))) {
            if !touched[cells(start.0, start.1)] || seen[cells(start.0, start.1)] {
                continue;
            }
            seen[cells(start.0, start.1)] = true;
            let (mut low, mut high, mut waiting) = (start, start, vec![start]);
            while let Some((x, y)) = waiting.pop() {
                (low, high) = ((low.0.min(x), low.1.min(y)), (high.0.max(x), high.1.max(y)));
                for (nx, ny) in neighbours(x, y, across, down) {
                    if touched[cells(nx, ny)] && !seen[cells(nx, ny)] {
                        seen[cells(nx, ny)] = true;
                        waiting.push((nx, ny));
                    }
                }
            }
            places.push(self.around(low, high));
        }
        joined(places, CELL)
    }

    /// The box around the pixels that changed in the squares from `low` to `high`.
    pub(super) fn around(&self, low: (u32, u32), high: (u32, u32)) -> Place {
        let (left, top) = (low.0 * SQUARE, low.1 * SQUARE);
        let (right, bottom) = (((high.0 + 1) * SQUARE).min(self.width), ((high.1 + 1) * SQUARE).min(self.height));
        let changed = |x: u32, y: u32| self.at(x, y);
        let x = (left..right).find(|x| (top..bottom).any(|y| changed(*x, y))).unwrap_or(left);
        let y = (top..bottom).find(|y| (left..right).any(|x| changed(x, *y))).unwrap_or(top);
        let last_x = (left..right).rev().find(|x| (top..bottom).any(|y| changed(*x, y))).unwrap_or(right - 1);
        let last_y = (top..bottom).rev().find(|y| (left..right).any(|x| changed(x, *y))).unwrap_or(bottom - 1);
        Place { x, y, width: last_x - x + 1, height: last_y - y + 1 }
    }
}

/// `places`, with each two whose boxes come within `apart` pixels of each other made one, until none do, in order down
/// and then across.
pub(super) fn joined(mut places: Vec<Place>, apart: u32) -> Vec<Place> {
    loop {
        let before = places.len();
        let mut kept: Vec<Place> = Vec::with_capacity(before);
        for place in places {
            match kept.iter_mut().find(|other| other.near(place, apart)) {
                Some(other) => *other = other.and(place),
                None => kept.push(place),
            }
        }
        places = kept;
        if places.len() == before {
            break;
        }
    }
    places.sort_by_key(|place| (place.y, place.x));
    places
}

impl Place {
    pub(super) fn right(self) -> u32 {
        self.x + self.width
    }

    pub(super) fn bottom(self) -> u32 {
        self.y + self.height
    }

    /// Whether the two boxes overlap or come within `apart` pixels of each other.
    pub(super) fn near(self, other: Place, apart: u32) -> bool {
        let close = |low: u32, high: u32| low <= high.saturating_add(apart);
        close(self.x, other.right())
            && close(other.x, self.right())
            && close(self.y, other.bottom())
            && close(other.y, self.bottom())
    }

    /// The box around both.
    pub(super) fn and(self, other: Place) -> Place {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Place { x, y, width: self.right().max(other.right()) - x, height: self.bottom().max(other.bottom()) - y }
    }
}

/// The squares around `(x, y)`, at its sides and corners, inside `across` by `down`.
fn neighbours(x: u32, y: u32, across: u32, down: u32) -> impl Iterator<Item = (u32, u32)> {
    let near = |at: u32, most: u32| at.saturating_sub(1)..=(at + 1).min(most - 1);
    near(y, down).flat_map(move |ny| near(x, across).map(move |nx| (nx, ny))).filter(move |near| *near != (x, y))
}

impl Difference {
    /// How many pixels changed, and in how many places, as `1,204 pixels in 3 places`, which a swatch of the colour
    /// they're tinted says are what changed.
    pub(crate) fn describe(&self) -> String {
        match (self.changed, self.places.len()) {
            (0, _) => "No pixels changed".to_string(),
            (1, _) => "1 pixel changed".to_string(),
            (changed, 1) => format!("{} pixels in 1 place", thousands(changed)),
            (changed, places) => format!("{} pixels in {places} places", thousands(changed)),
        }
    }
}

/// `number` with a comma between each three digits.
fn thousands(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A colour of the theme's, as a pixel.
pub(crate) fn colour(colour: Hsla) -> Rgba<u8> {
    let colour = gpui_kit::Rgba::from(colour);
    Rgba([channel(colour.r * 255.), channel(colour.g * 255.), channel(colour.b * 255.), channel(colour.a * 255.)])
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to what a channel holds first")]
fn channel(value: f32) -> u8 {
    value.round().clamp(0., 255.) as u8
}
