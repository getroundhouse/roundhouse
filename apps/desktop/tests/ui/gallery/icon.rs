//! The app icon, in each style and at every size macOS keeps a drawing for, checked against its images in the
//! style's iconset in `apps/desktop/icons/`. `iconutil` packs the app's, `working.iconset`, into the `.icns` the app
//! is packaged with, and `tools/package/ico.py` packs five of its drawings into `AppIcon.ico` beside it, for Windows,
//! which `tools/package/test_ico.py` fails on once the drawings change, until it's made again.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{
    Context, HeadlessAppContext, Hsla, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    Window, black, div, px, size, white,
};
use image::{Rgba, RgbaImage};
use look::{IconSize, IconStyle};

use super::approved::{self, Verdict};
use super::{UPDATE, headless};

/// The iconset's files, and the drawing each holds. macOS names them by points and scale, so some share a drawing.
const FILES: [(&str, IconSize); 10] = [
    ("icon_16x16.png", IconSize::Px16),
    ("icon_16x16@2x.png", IconSize::Px32),
    ("icon_32x32.png", IconSize::Px32),
    ("icon_32x32@2x.png", IconSize::Px64),
    ("icon_128x128.png", IconSize::Px128),
    ("icon_128x128@2x.png", IconSize::Px256),
    ("icon_256x256.png", IconSize::Px256),
    ("icon_256x256@2x.png", IconSize::Px512),
    ("icon_512x512.png", IconSize::Px512),
    ("icon_512x512@2x.png", IconSize::Px1024),
];

#[test]
fn every_size_of_the_icon_looks_as_approved() {
    let update = std::env::var_os(UPDATE).is_some();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/gallery");
    std::fs::create_dir_all(&out).unwrap();
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx = headless::app(&text_system);
    let mut problems = Vec::new();
    for style in IconStyle::ALL {
        for size in IconSize::ALL {
            let drawn = draw(&mut cx, style, size);
            let pixels = u32::from(size.pixels());
            assert_eq!(drawn.dimensions(), (pixels, pixels), "drawn at 2x, so each point is two pixels");
            drawn.save(out.join(format!("icon-{}-{pixels}.png", name(style)))).unwrap();
            let files: Vec<&str> = FILES.iter().filter(|(_, s)| *s == size).map(|(file, _)| *file).collect();
            if files.is_empty() {
                problems.push(format!("the {pixels}px icon has no file"));
            }
            for file in files {
                let path = iconset(style).join(file);
                let which = format!("{}.iconset/{file}", name(style));
                if update {
                    approved::accept_file(&path, &drawn);
                    continue;
                }
                match approved::compare_file(&path, &drawn) {
                    Verdict::Matches => {}
                    Verdict::Differs => problems.push(format!("{which} no longer matches the {pixels}px icon")),
                    Verdict::Missing => problems.push(format!("{which} is missing")),
                }
            }
        }
    }
    problems.extend(unknown_files().into_iter().map(|path| format!("{path} isn't one of the icon's files")));
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each size looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it.",
        problems.join("\n")
    );
}

/// The icon over a backdrop of one colour.
struct Icon {
    style: IconStyle,
    size: IconSize,
    backdrop: Hsla,
}

impl Render for Icon {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(self.backdrop).child(look::icon(self.style, self.size, cx))
    }
}

/// Clear around its tile. The renderer only draws opaque pictures, so it's drawn over black and over white, and
/// the two give back how much of each pixel the icon covers, and in what colour.
fn draw(cx: &mut HeadlessAppContext, style: IconStyle, icon: IconSize) -> RgbaImage {
    unblend(&over(cx, style, icon, black()), &over(cx, style, icon, white()))
}

/// In a window exactly as big as the icon.
fn over(cx: &mut HeadlessAppContext, style: IconStyle, icon: IconSize, backdrop: Hsla) -> RgbaImage {
    let points = px(f32::from(icon.pixels()) / 2.0);
    let picture = headless::draw(cx, size(points, points), move |_| Icon { style, size: icon, backdrop });
    assert!(picture.settled, "the {}px {} icon keeps changing", icon.pixels(), name(style));
    picture.image
}

/// A pixel the icon covers by `a` comes out as `a × colour` over black, and `a × colour + 1 − a` over white. So the
/// difference between them gives `a`, and the pixel over black then gives the colour.
fn unblend(on_black: &RgbaImage, on_white: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(on_black.width(), on_black.height(), |x, y| {
        let (dark, light) = (on_black.get_pixel(x, y).0, on_white.get_pixel(x, y).0);
        let gap = (0..3).map(|i| f32::from(light[i]) - f32::from(dark[i])).sum::<f32>() / 3.0;
        let alpha = (255.0 - gap).clamp(0.0, 255.0);
        if alpha < 0.5 {
            return Rgba([0, 0, 0, 0]);
        }
        let channel = |i: usize| byte(f32::from(dark[i]) * 255.0 / alpha);
        Rgba([channel(0), channel(1), channel(2), byte(alpha)])
    })
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to a byte's range first")]
fn byte(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// What a style's iconset is called.
fn name(style: IconStyle) -> &'static str {
    match style {
        IconStyle::Idle => "idle",
        IconStyle::Working => "working",
    }
}

fn icons() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons")
}

fn iconset(style: IconStyle) -> PathBuf {
    icons().join(format!("{}.iconset", name(style)))
}

/// Files in `icons/` that nothing draws, so nothing checks them: an iconset for no style, or a file in a style's
/// iconset for no size.
fn unknown_files() -> Vec<String> {
    let names = |dir: PathBuf| -> Vec<String> {
        let entries = std::fs::read_dir(dir).into_iter().flatten().map(|entry| entry.unwrap().file_name());
        entries.map(|name| name.to_string_lossy().into_owned()).collect()
    };
    let styles: Vec<String> = IconStyle::ALL.iter().map(|style| format!("{}.iconset", name(*style))).collect();
    let mut unknown: Vec<String> = names(icons()).into_iter().filter(|dir| !styles.contains(dir)).collect();
    for style in IconStyle::ALL {
        let files = names(iconset(style)).into_iter().filter(|file| !FILES.iter().any(|(known, _)| known == file));
        unknown.extend(files.map(|file| format!("{}.iconset/{file}", name(style))));
    }
    unknown
}
