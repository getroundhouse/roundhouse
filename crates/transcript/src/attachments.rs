//! What a prompt carries: each image drawn small until it's opened, and each other file as a chip. An image a tool
//! sent back is drawn whole, in what its call opens to. An image's bytes come from the hub the first time a row draws
//! it, and stay for as long as the transcript does.

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::attachment::{
    Attachment as Card, AttachmentContent, AttachmentDescription, AttachmentMedia, AttachmentTitle,
};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Image, ImageFormat, InteractiveElement, IntoElement, ObjectFit, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, StyledImage as _, TestSupportExt as _, WeakEntity, div, img, px,
};
use image::ImageDecoder as _;
use image::metadata::Orientation;
use look::Hand as _;
use protocol::attachment::{Attachment, MediaType, describe_size};
use protocol::model::Seq;

use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::{Model, Transcript};

/// How tall an image is drawn until it's opened: big enough to recognise a screenshot, small enough that a prompt
/// with several doesn't push the reply off screen.
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
    opened: bool,
}

pub(crate) fn carried(m: &Model, attachments: &[Attachment]) -> Vec<Carried> {
    attachments
        .iter()
        .map(|attachment| {
            let key = ItemKey::Attachment(attachment.id.clone());
            Carried {
                picture: m.pictures.get(&attachment.id).cloned(),
                opened: m.folds.is_open(&key, Fold::Picture),
                key,
                attachment: attachment.clone(),
            }
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

/// Images first, in a row that wraps, with an opened one on a line of its own, then the other files as chips.
pub(crate) fn view(
    seq: Option<Seq>,
    carried: &[Carried],
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Option<AnyElement> {
    if carried.is_empty() {
        return None;
    }
    let (images, files): (Vec<&Carried>, Vec<&Carried>) =
        carried.iter().partition(|file| file.attachment.media_type.is_image());
    let images = images.into_iter().map(|file| picture(seq, file, this, cx));
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

fn picture(seq: Option<Seq>, file: &Carried, this: &WeakEntity<Transcript>, cx: &App) -> AnyElement {
    let Some((image, width, height)) = ready(file) else { return not_ready(file, cx) };
    let frame = frame(file, cx);
    let (width, height) = (pixels(width), pixels(height));
    let frame = match seq {
        Some(seq) => {
            let (this, key, opened) = (this.clone(), file.key.clone(), file.opened);
            frame.hand().on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), Fold::Picture, !opened, cx));
            })
        }
        None => frame,
    };
    if file.opened {
        // A line of its own, as wide as the image, up to what it's in, at its own shape. Its own width, rather than a
        // share of what it's in, is what lets a prompt that fits its contents grow to show it.
        let frame = frame.w(px(width)).max_w_full().aspect_ratio(width / height);
        let picture = frame.child(img(image).size_full().object_fit(ObjectFit::Contain)).test_support();
        return div().w_full().child(picture).into_any_element();
    }
    let frame =
        frame.h(px(THUMBNAIL_HEIGHT)).w(px((THUMBNAIL_HEIGHT * width / height).clamp(32., THUMBNAIL_MAX_WIDTH)));
    frame.child(img(image).size_full().object_fit(ObjectFit::Cover)).test_support().into_any_element()
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

#[expect(clippy::cast_precision_loss, reason = "an image's side is far below where an f32 loses whole pixels")]
fn pixels(side: u32) -> f32 {
    side as f32
}
