//! The app icon, drawn at every size macOS keeps a drawing for, and checked against its images in
//! `apps/desktop/AppIcon.iconset/`, which `iconutil` packs into the `.icns` the app is packaged with.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::assets::Assets;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, HeadlessAppContext, Hsla, IntoElement, ParentElement as _, PlatformTextSystem,
    Render, Styled as _, Window, WindowBounds, WindowOptions, black, div, point, px, size, white,
};
use image::{Rgba, RgbaImage};
use look::IconSize;

use super::UPDATE;
use super::approved::{self, Verdict};

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
    let mut cx = headless();
    let mut problems = Vec::new();
    for size in IconSize::ALL {
        let drawn = draw(&mut cx, size);
        let pixels = u32::from(size.pixels());
        assert_eq!(drawn.dimensions(), (pixels, pixels), "drawn at 2x, so each point is two pixels");
        drawn.save(out.join(format!("icon-{pixels}.png"))).unwrap();
        let files: Vec<&str> = FILES.iter().filter(|(_, s)| *s == size).map(|(name, _)| *name).collect();
        if files.is_empty() {
            problems.push(format!("the {pixels}px icon has no file"));
        }
        for name in files {
            let path = iconset().join(name);
            if update {
                approved::accept_file(&path, &drawn);
                continue;
            }
            match approved::compare_file(&path, &drawn) {
                Verdict::Matches => {}
                Verdict::Differs => problems.push(format!("{name} no longer matches the {pixels}px icon")),
                Verdict::Missing => problems.push(format!("{name} is missing")),
            }
        }
    }
    problems.extend(unknown_files().into_iter().map(|name| format!("{name} isn't one of the icon's files")));
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each size looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it.",
        problems.join("\n")
    );
}

/// The icon over a backdrop of one colour.
struct Icon {
    size: IconSize,
    backdrop: Hsla,
}

impl Render for Icon {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(self.backdrop).child(look::icon(self.size, cx))
    }
}

fn headless() -> HeadlessAppContext {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    cx.update(look::apply);
    cx
}

/// Clear around its tile. The renderer only draws opaque pictures, so it's drawn over black and over white, and
/// the two give back how much of each pixel the icon covers, and in what colour.
fn draw(cx: &mut HeadlessAppContext, icon: IconSize) -> RgbaImage {
    unblend(&over(cx, icon, black()), &over(cx, icon, white()))
}

/// In a window exactly as big as the icon.
fn over(cx: &mut HeadlessAppContext, icon: IconSize, backdrop: Hsla) -> RgbaImage {
    let points = px(f32::from(icon.pixels()) / 2.0);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(points, points)))),
        focus: false,
        show: false,
        ..Default::default()
    };
    let handle = cx.update(|cx| cx.open_window(options, |_, cx| cx.new(|_| Icon { size: icon, backdrop }))).unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx)).unwrap();
    let image = cx.capture_screenshot(handle.into()).unwrap();
    cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    image
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

fn iconset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("AppIcon.iconset")
}

/// Files in the iconset that nothing draws, so nothing checks them.
fn unknown_files() -> Vec<String> {
    let entries = std::fs::read_dir(iconset()).into_iter().flatten().map(|entry| entry.unwrap().file_name());
    entries
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !FILES.iter().any(|(known, _)| known == name))
        .collect()
}
