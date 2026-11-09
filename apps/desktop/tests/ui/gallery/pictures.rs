//! Made-up images for the screens, drawn from rectangles so they come out the same on every run.

#![cfg(test)]

use std::io::Cursor;

use image::{Rgba, RgbaImage};

const PANE: Rgba<u8> = Rgba([0x15, 0x13, 0x12, 0xff]);
const SIDEBAR: Rgba<u8> = Rgba([0x1a, 0x17, 0x15, 0xff]);
const EDGE: Rgba<u8> = Rgba([0x2a, 0x26, 0x24, 0xff]);
const TEXT: Rgba<u8> = Rgba([0x8a, 0x84, 0x7f, 0xff]);
const FAINT: Rgba<u8> = Rgba([0x4a, 0x45, 0x41, 0xff]);
const GREEN: Rgba<u8> = Rgba([0x3f, 0xb9, 0x50, 0xff]);

/// A narrow sidebar where a pull request's badge runs over a workspace's name, as someone might paste to show a bug.
pub(crate) fn overlapping_badge() -> Vec<u8> {
    let mut image = RgbaImage::from_pixel(720, 450, PANE);
    fill(&mut image, (0, 0), (250, 450), SIDEBAR);
    fill(&mut image, (250, 0), (1, 450), EDGE);
    for (row, name) in [120, 96, 150, 104, 130].into_iter().enumerate() {
        let y = 60 + 40 * u32::try_from(row).unwrap();
        fill(&mut image, (22, y), (12, 12), FAINT);
        fill(&mut image, (44, y + 2), (name, 8), TEXT);
    }
    // The badge, over the end of the second row's name, where it should sit after it.
    fill(&mut image, (118, 96), (58, 20), GREEN);
    fill(&mut image, (128, 103), (38, 6), PANE);
    for (line, width) in [380, 410, 300, 360].into_iter().enumerate() {
        fill(&mut image, (290, 60 + 28 * u32::try_from(line).unwrap()), (width, 8), FAINT);
    }
    encode(&image)
}

/// A terminal's last lines, with the failing one in red.
pub(crate) fn terminal() -> Vec<u8> {
    let mut image = RgbaImage::from_pixel(640, 360, Rgba([0x0e, 0x0d, 0x0c, 0xff]));
    for (line, (width, color)) in
        [(300, TEXT), (420, TEXT), (260, TEXT), (480, Rgba([0xd0, 0x4a, 0x4a, 0xff]))].into_iter().enumerate()
    {
        fill(&mut image, (24, 30 + 26 * u32::try_from(line).unwrap()), (width, 9), color);
    }
    encode(&image)
}

fn fill(image: &mut RgbaImage, (x, y): (u32, u32), (width, height): (u32, u32), color: Rgba<u8>) {
    for py in y..(y + height).min(image.height()) {
        for px in x..(x + width).min(image.width()) {
            image.put_pixel(px, py, color);
        }
    }
}

fn encode(image: &RgbaImage) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}
