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

/// A narrow sidebar where a pull request's badge runs over a workspace's name, as someone might paste to show a bug,
/// `height` pixels tall, or, unless `overlapping`, the same sidebar once that's fixed. Its rows end above 240.
pub(crate) fn badge(height: u32, overlapping: bool) -> Vec<u8> {
    let mut image = RgbaImage::from_pixel(720, height, PANE);
    fill(&mut image, (0, 0), (250, height), SIDEBAR);
    fill(&mut image, (250, 0), (1, height), EDGE);
    for (row, name) in [120, 96, 150, 104, 130].into_iter().enumerate() {
        let y = 60 + 40 * u32::try_from(row).unwrap();
        fill(&mut image, (22, y), (12, 12), FAINT);
        fill(&mut image, (44, y + 2), (name, 8), TEXT);
    }
    // The badge, over the end of the second row's name, or after it, where it should sit.
    let x = if overlapping { 118 } else { 148 };
    fill(&mut image, (x, 96), (58, 20), GREEN);
    fill(&mut image, (x + 10, 103), (38, 6), PANE);
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

/// A page of signals in a browser, each a dot and a name, as the agent's screenshot of it would show it, with the fourth
/// at stop showing red.
pub(crate) fn signal_board() -> Vec<u8> {
    const RED: Rgba<u8> = Rgba([0xd0, 0x4a, 0x4a, 0xff]);
    const AMBER: Rgba<u8> = Rgba([0xd8, 0x9a, 0x3a, 0xff]);
    let mut image = RgbaImage::from_pixel(600, 340, PANE);
    fill(&mut image, (0, 0), (600, 36), SIDEBAR);
    fill(&mut image, (0, 36), (600, 1), EDGE);
    for (x, width) in [(16, 10), (34, 10), (52, 10), (90, 260)] {
        fill(&mut image, (x, 13), (width, 10), FAINT);
    }
    for (row, (color, name)) in
        [(GREEN, 140), (AMBER, 110), (GREEN, 160), (RED, 120), (RED, 130), (GREEN, 100)].into_iter().enumerate()
    {
        let y = 66 + 42 * u32::try_from(row).unwrap();
        fill(&mut image, (32, y), (16, 16), color);
        fill(&mut image, (64, y + 4), (name, 8), TEXT);
        fill(&mut image, (440, y + 4), (120, 8), FAINT);
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
