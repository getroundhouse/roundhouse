//! What a prompt carries: each image drawn small, which a click opens over the window, and each other file as a chip.
//! An image a tool sent back is drawn whole, in what its call opens to. An image's bytes come from the hub the first
//! time a row draws it, and stay for as long as the transcript does.

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::attachment::{
    Attachment as Card, AttachmentContent, AttachmentDescription, AttachmentMedia, AttachmentTitle,
};
use gpui_kit::component::dialog::DialogClose;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, TITLE_BAR_HEIGHT, WindowExt as _, h_flex, window_paddings,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Image, ImageFormat, InteractiveElement, IntoElement, ObjectFit, ParentElement,
    Pixels, Size, Stateful, StatefulInteractiveElement, Styled, StyledImage as _, TestSupportExt as _, WeakEntity,
    Window, div, img, px, size,
};
use image::ImageDecoder as _;
use image::metadata::Orientation;
use look::{Hand as _, IMAGE_EDGE, fit, pixels};
use protocol::attachment::{Attachment, MediaType, describe_size};
use protocol::model::Seq;

use crate::item_key::ItemKey;
use crate::transcript::{Model, Transcript};

/// How tall a prompt's image is drawn: big enough to recognise a screenshot, small enough that a prompt with several
/// doesn't push the reply off screen.
const THUMBNAIL_HEIGHT: f32 = 120.;
const THUMBNAIL_MAX_WIDTH: f32 = 320.;
/// For an image whose size isn't known yet, the width of a screenshot of a wide window.
const THUMBNAIL_GUESSED_WIDTH: f32 = 190.;

/// An attached image, as far as the transcript has it.
#[derive(Clone)]
pub(crate) enum Picture {
    /// Asked for, and not back yet.
    Loading,
    /// `width` and `height` are its pixels, the way it's drawn.
    Ready { image: Arc<Image>, width: u32, height: u32 },
    /// The hub couldn't give it, or it couldn't be read.
    Unreadable,
}

impl Picture {
    /// Reads only the image's header, for its size, and leaves decoding it to GPUI when it's drawn.
    pub(crate) fn read(media_type: MediaType, bytes: Vec<u8>) -> Picture {
        let Some(format) = image_format(media_type) else { return Picture::Unreadable };
        match size_of(&bytes) {
            Some((width, height)) => {
                Picture::Ready { image: Arc::new(Image::from_bytes(format, bytes)), width, height }
            }
            None => Picture::Unreadable,
        }
    }
}

/// How GPUI draws an attachment of this kind, if it's an image.
pub fn image_format(media_type: MediaType) -> Option<ImageFormat> {
    match media_type {
        MediaType::Png => Some(ImageFormat::Png),
        MediaType::Jpeg => Some(ImageFormat::Jpeg),
        MediaType::Gif => Some(ImageFormat::Gif),
        MediaType::Webp => Some(ImageFormat::Webp),
        MediaType::Text | MediaType::Other | MediaType::Unknown => None,
    }
}

/// Its width and height as drawn, which a photo's orientation can swap.
fn size_of(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut decoder = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    let turned = matches!(
        decoder.orientation().unwrap_or(Orientation::NoTransforms),
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    );
    (width > 0 && height > 0).then_some(if turned { (height, width) } else { (width, height) })
}

/// One file a prompt carries, with what drawing it needs, read while the model is held.
pub(crate) struct Carried {
    attachment: Attachment,
    key: ItemKey,
    /// `None` for an image nobody has asked the hub for yet, and for a file that isn't an image.
    picture: Option<Picture>,
}

pub(crate) fn carried(m: &Model, attachments: &[Attachment]) -> Vec<Carried> {
    attachments
        .iter()
        .map(|attachment| Carried {
            picture: m.pictures.get(&attachment.id).cloned(),
            key: ItemKey::Attachment(attachment.id.clone()),
            attachment: attachment.clone(),
        })
        .collect()
}

/// Asks the hub for each image here it hasn't been asked for, once drawing has finished. `seq` is the row's, to
/// measure it again when one arrives, and `None` for a prompt that's pending, when every image is already here.
pub(crate) fn fetch_missing<'a>(
    model: &Rc<RefCell<Model>>,
    this: &WeakEntity<Transcript>,
    seq: Option<Seq>,
    carried: impl IntoIterator<Item = &'a Carried>,
    cx: &mut App,
) {
    let missing = carried.into_iter().filter(|file| file.attachment.media_type.is_image() && file.picture.is_none());
    for file in missing {
        model.borrow_mut().pictures.insert(file.attachment.id.clone(), Picture::Loading);
        let (this, attachment) = (this.clone(), file.attachment.clone());
        cx.defer(move |cx| {
            let _ = this.update(cx, |t, cx| t.load_picture(seq, attachment, cx));
        });
    }
}

/// Images first, in a row that wraps, then the other files as chips.
pub(crate) fn view(carried: &[Carried], cx: &App) -> Option<AnyElement> {
    if carried.is_empty() {
        return None;
    }
    let (images, files): (Vec<&Carried>, Vec<&Carried>) =
        carried.iter().partition(|file| file.attachment.media_type.is_image());
    let images = images.into_iter().map(|file| thumbnail(file, cx));
    let files = files.into_iter().map(|file| file_card(file.key.element("file"), &file.attachment).into_any_element());
    Some(h_flex().w_full().flex_wrap().items_start().gap_2().children(images).children(files).into_any_element())
}

/// An image drawn whole, as wide as it is, up to the column, at its own shape, with nothing to click. For an image a
/// tool sent back, which opens with its call.
pub(crate) fn whole(file: &Carried, cx: &App) -> AnyElement {
    let Some((image, width, height)) = ready(file) else { return not_ready(file, cx) };
    let (width, height) = (pixels(width), pixels(height));
    let shown = frame(file, cx)
        .w_full()
        .max_w(px(width))
        .aspect_ratio(width / height)
        .child(img(image).size_full().object_fit(ObjectFit::Contain))
        .test_support();
    // In a row, since in a column the layout takes its height from the column's width rather than its own, which
    // leaves a gap under an image narrower than the column.
    h_flex().w_full().child(shown).into_any_element()
}

/// Small, and cropped to fill its place, for a click to open over the window.
fn thumbnail(file: &Carried, cx: &App) -> AnyElement {
    let Some((image, width, height)) = ready(file) else { return not_ready(file, cx) };
    let to_open = image.clone();
    let shape = pixels(width) / pixels(height);
    frame(file, cx)
        .hand()
        .on_click(move |_, window, cx| open_over_the_window(to_open.clone(), (width, height), window, cx))
        .h(px(THUMBNAIL_HEIGHT))
        .w(px((THUMBNAIL_HEIGHT * shape).clamp(32., THUMBNAIL_MAX_WIDTH)))
        .child(img(image).size_full().object_fit(ObjectFit::Cover))
        .test_support()
        .into_any_element()
}

/// Opens an image in a dialog, as big as it is, up to what fits in the window at its own shape, so the chat doesn't
/// move under it however far from the bottom its prompt is. Escape or a click closes it, on the image or around it.
/// `covers` is its pixels, turned as its orientation says.
fn open_over_the_window(image: Arc<Image>, covers: (u32, u32), window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, cx| {
        // Drawn again as the window is resized, so it always fits.
        let shown = fitted(covers, room(window));
        // The dialog's own corners, which its edge is drawn on, less the edge, so the image's meet them.
        let corners = cx.theme().radius_lg - px(IMAGE_EDGE);
        let picture = div()
            .id("opened-picture")
            .w(shown.width)
            .h(shown.height)
            .hand()
            .child(img(image.clone()).size_full().rounded(corners).object_fit(ObjectFit::Contain))
            .test_support();
        // The dialog's own padding and the least height it gives, which a short image would leave a strip under, are
        // let go, so the dialog is the image and its edge.
        dialog
            .p_0()
            .min_h(px(0.))
            .w(shown.width + px(2. * IMAGE_EDGE))
            .bg(cx.theme().muted)
            .centered()
            .close_button(false)
            .child(DialogClose::new().child(picture))
    });
}

/// The room in the window for an opened image: all of it but the window's own frame, where it draws one, and as much
/// around it as the strip along the top that the kit's dialogs leave to the window's buttons and to dragging it.
fn room(window: &Window) -> Size<Pixels> {
    let frame = window_paddings(window);
    let clear = TITLE_BAR_HEIGHT * 2.;
    let viewport = window.viewport_size();
    size(viewport.width - frame.left - frame.right - clear, viewport.height - frame.top - frame.bottom - clear)
}

/// How big an image `covers` pixels is drawn, inside its edge, to fit in `room`.
fn fitted((width, height): (u32, u32), room: Size<Pixels>) -> Size<Pixels> {
    let scale = fit((width, height), room);
    size(px(pixels(width) * scale), px(pixels(height) * scale))
}

fn frame(file: &Carried, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(file.key.element("picture"))
        .flex_none()
        .rounded(px(6.))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
}

fn ready(file: &Carried) -> Option<(Arc<Image>, u32, u32)> {
    match &file.picture {
        Some(Picture::Ready { image, width, height }) => Some((image.clone(), *width, *height)),
        Some(Picture::Loading | Picture::Unreadable) | None => None,
    }
}

/// Where an image will be, the size of a thumbnail, saying so if it can't be shown.
fn not_ready(file: &Carried, cx: &App) -> AnyElement {
    let unreadable = matches!(file.picture, Some(Picture::Unreadable));
    frame(file, cx)
        .h(px(THUMBNAIL_HEIGHT))
        .w(px(THUMBNAIL_GUESSED_WIDTH))
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(look::faint())
        .when(unreadable, |d| d.child(format!("Couldn't show {}", file.attachment.name)))
        .into_any_element()
}

/// A file that isn't drawn, as its name and size, the same in the transcript and the composer.
pub fn file_card(id: impl Into<ElementId>, attachment: &Attachment) -> Card {
    let name = attachment.name.clone();
    Card::new()
        .id(id)
        .small()
        .tooltip(name.clone())
        .media(AttachmentMedia::new().child(Icon::new(IconName::FileText)))
        .content(
            AttachmentContent::new()
                .title(AttachmentTitle::new(name))
                .description(AttachmentDescription::new(describe_size(attachment.size_bytes))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Room 932 by 732, which leaves 930 by 730 for an image inside its edge.
    fn in_the_room(width: u32, height: u32) -> Size<Pixels> {
        fitted((width, height), size(px(932.), px(732.)))
    }

    #[test]
    fn an_image_that_fits_is_as_big_as_it_is() {
        assert_eq!(in_the_room(400, 300), size(px(400.), px(300.)));
        assert_eq!(in_the_room(930, 730), size(px(930.), px(730.)), "to the edge of the room");
    }

    #[test]
    fn one_too_big_shrinks_to_fit_at_its_own_shape() {
        assert_eq!(in_the_room(9300, 100), size(px(930.), px(10.)), "too wide");
        assert_eq!(in_the_room(100, 7300), size(px(10.), px(730.)), "too tall");
        assert_eq!(in_the_room(1860, 2920), size(px(465.), px(730.)), "both, by whichever is further over");
    }
}
