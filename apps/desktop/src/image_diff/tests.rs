use std::io::Cursor;

use client_core::ClientError;
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::model::FileContents;

use super::{Side, side};
use crate::file_view::hash;

fn png(width: u32) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let image = image::RgbaImage::from_pixel(width, 4, image::Rgba([200, 30, 30, 255]));
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// What the hub gives for a PNG of `data`.
fn image(data: &[u8]) -> FileContents {
    FileContents::Image { media_type: MediaType::Png, data: AttachmentBytes::new(data.to_vec()) }
}

/// So an image the diff reads again each turn isn't decoded again, and stays as it's drawn, unless it changed.
#[test]
fn an_image_read_again_is_left_as_it_is_unless_it_changed() {
    let (shown, changed) = (png(6), png(8));

    assert!(side(Ok(image(&shown)), Some(hash(&shown))).is_none(), "the same bytes again");
    let Some(Side::Image(read)) = side(Ok(image(&changed)), Some(hash(&shown))) else { panic!("it changed") };
    assert_eq!((read.width, read.hash), (8, hash(&changed)));
    assert!(side(Err(ClientError::Disconnected), None).is_none(), "lost with the connection");
}
