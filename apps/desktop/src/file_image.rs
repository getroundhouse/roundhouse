//! An image the file tab draws, from a file as it is or as a commit held it: its first frame, decoded once, off the
//! main thread, and turned as a photo's orientation says, what kind of image it is and how big, and how it's drawn.
//! It's fitted to the room there is, both ways and at its own shape, and never drawn bigger than it is, on a
//! checkerboard that shows what's clear in it. Each window that draws it keeps a copy of it on the GPU until it's
//! released.

use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    AnyElement, App, AvailableSpace, Div, IntoElement, ObjectFit, ParentElement as _, RenderImage, Styled,
    StyledImage as _, canvas, checkerboard, div, img, px,
};
use image::{DynamicImage, Frame, ImageDecoder as _, ImageReader, RgbaImage};
use look::{IMAGE_EDGE, fit, pixels};
use protocol::attachment::{MediaType, describe_size};

/// How big each square of the checkerboard behind an image is.
const SQUARE: f32 = 8.;

#[derive(Clone)]
pub(crate) struct FileImage {
    /// Its first frame, in GPUI's order of channels.
    picture: Arc<RenderImage>,
    media_type: MediaType,
    /// Its pixels, the way it's drawn.
    pub(crate) width: u32,
    pub(crate) height: u32,
    bytes: u64,
    /// A hash of its bytes, as [`crate::file_view::hash`] makes it, which tells an image read again that hasn't
    /// changed before it's decoded.
    pub(crate) hash: u64,
}

impl FileImage {
    /// Decodes `data`, whose bytes hash to `hash`, which takes a while for a big image, so it's for a background thread.
    /// `None` when it isn't a kind that's drawn, or it can't be decoded, or decoding it would take more memory than
    /// the image crate allows. Only its first frame is decoded, which is all that's drawn of one that moves.
    pub(crate) fn read(media_type: MediaType, data: &[u8], hash: u64) -> Option<FileImage> {
        let format = match media_type {
            MediaType::Png => image::ImageFormat::Png,
            MediaType::Jpeg => image::ImageFormat::Jpeg,
            MediaType::Gif => image::ImageFormat::Gif,
            MediaType::Webp => image::ImageFormat::WebP,
            MediaType::Text | MediaType::Other | MediaType::Unknown => return None,
        };
        let mut decoder = ImageReader::with_format(Cursor::new(data), format).into_decoder().ok()?;
        let orientation = decoder.orientation().ok()?;
        let mut frame = DynamicImage::from_decoder(decoder).ok()?;
        frame.apply_orientation(orientation);
        let mut pixels = frame.into_rgba8();
        let (width, height) = pixels.dimensions();
        swap_red_and_blue(&mut pixels);
        let picture = Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
        let bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        Some(FileImage { picture, media_type, width, height, bytes, hash })
    }

    /// Its pixels, red first, as an image is read, to compare it with another. Copying a big one takes a while, so it's
    /// for a background thread.
    pub(crate) fn pixels(&self) -> Option<RgbaImage> {
        let mut pixels = RgbaImage::from_raw(self.width, self.height, self.picture.as_bytes(0)?.to_vec())?;
        swap_red_and_blue(&mut pixels);
        Some(pixels)
    }

    /// What's drawn of it, for a test to find on the GPU.
    #[cfg(feature = "testing")]
    pub(crate) fn picture(&self) -> Arc<RenderImage> {
        self.picture.clone()
    }

    /// Lets each window's copy of it on the GPU go, as [`release`] says. The rest goes once the last of it is dropped.
    pub(crate) fn release(&self, cx: &mut App) {
        release(self.picture.clone(), cx);
    }

    /// What kind it is, its size in pixels and in bytes, as `PNG · 1440 × 900 · 214 KB`.
    pub(crate) fn describe(&self) -> String {
        format!("{} · {}", self.kind(), self.sizes())
    }

    /// Its size in pixels and in bytes, as `1440 × 900 · 214 KB`.
    pub(crate) fn sizes(&self) -> String {
        format!("{} × {} · {}", self.width, self.height, describe_size(self.bytes))
    }

    /// Whether it's the same kind of image as `other`.
    pub(crate) fn same_kind(&self, other: &FileImage) -> bool {
        self.kind() == other.kind()
    }

    fn kind(&self) -> &'static str {
        match self.media_type {
            MediaType::Png => "PNG",
            MediaType::Jpeg => "JPEG",
            MediaType::Gif => "GIF",
            MediaType::Webp => "WebP",
            MediaType::Text | MediaType::Other | MediaType::Unknown => "Image",
        }
    }

    /// It on its checkerboard, `scale` times its size, inside its edge.
    pub(crate) fn whole(&self, scale: f32, cx: &App) -> Div {
        edged(on_checkerboard(self.picture.clone(), (self.width, self.height), scale, cx), cx)
    }

    /// It at the top left of a box `scale` times the size of `covers`, such as what two images cover between them, and
    /// the edge around that, so each pixel of it is where the same pixel of the other is.
    pub(crate) fn within(&self, covers: (u32, u32), scale: f32, cx: &App) -> Div {
        let (width, height) = covers;
        let edged = |side: u32| px(pixels(side) * scale + 2. * IMAGE_EDGE);
        div().flex_none().w(edged(width)).h(edged(height)).child(self.whole(scale, cx))
    }
}

/// Lets each window's copy of `picture` on the GPU go, once the window being updated, which GPUI leaves out, is back
/// among the rest.
pub(crate) fn release(picture: Arc<RenderImage>, cx: &mut App) {
    cx.defer(move |cx| cx.drop_image(picture, None));
}

/// GPUI draws what it's given as blue, green, red and alpha, and an image is read as red, green, blue and alpha.
fn swap_red_and_blue(pixels: &mut RgbaImage) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

/// `picture`, of `covers` pixels, `scale` times its size, over a checkerboard that what's clear in it shows, in a box
/// what's drawn over it can be laid in from its top left.
pub(crate) fn on_checkerboard(picture: Arc<RenderImage>, covers: (u32, u32), scale: f32, cx: &App) -> Div {
    let drawn = img(picture).absolute().top_0().left_0().size_full().object_fit(ObjectFit::Fill);
    frame(covers, scale).relative().child(checkered(cx)).child(drawn)
}

/// A box `scale` times the size of something `width` by `height` pixels.
fn frame((width, height): (u32, u32), scale: f32) -> Div {
    div().w(px(pixels(width) * scale)).h(px(pixels(height) * scale))
}

/// A checkerboard filling what it's in, which what's clear in an image drawn over it shows.
fn checkered(cx: &App) -> Div {
    let theme = cx.theme();
    div().size_full().bg(theme.muted).child(div().size_full().bg(checkerboard(theme.border, SQUARE)))
}

/// `drawn` inside a thin edge, so an image shows where it ends even when its own colour at its sides is the pane's.
pub(crate) fn edged(drawn: Div, cx: &App) -> Div {
    div().flex_none().border_1().border_color(cx.theme().border).child(drawn)
}

/// Where in the room what's fitted to it is put, down: across, it's always in the middle.
#[derive(Clone, Copy)]
pub(crate) enum Placed {
    Top,
    Middle,
}

/// What `draw` makes at the scale that fits `covers` pixels into the room this element is given, inside an edge, and
/// never bigger than they are. Fitting takes the room's size, which layout alone can't give, so it's laid out once
/// that's known, and the element takes no room of its own, so it's given a size by what it's in.
pub(crate) fn fitted(
    covers: (u32, u32),
    placed: Placed,
    draw: impl FnOnce(f32, &App) -> Div + 'static,
) -> impl IntoElement + Styled {
    canvas(
        move |bounds, window, cx| {
            let row = h_flex().size_full().justify_center();
            let row = match placed {
                Placed::Top => row.items_start(),
                Placed::Middle => row.items_center(),
            };
            let mut element: AnyElement = row.child(draw(fit(covers, bounds.size), cx)).into_any_element();
            element.prepaint_as_root(bounds.origin, bounds.size.map(AvailableSpace::Definite), window, cx);
            element
        },
        |_, mut element, window, cx| element.paint(window, cx),
    )
}
