//! The screens' approved images, which the spec keeps in `docs/spec/screens/` and shows in `screens.md`.

#![cfg(test)]

use std::path::PathBuf;

use image::RgbaImage;

pub(crate) enum Verdict {
    Matches,
    Differs,
    Missing,
}

/// Exact, because the same screen draws the same pixels every time on one machine, so any difference is a change.
pub(crate) fn compare(name: &str, drawn: &RgbaImage) -> Verdict {
    let path = path(name);
    if !path.exists() {
        return Verdict::Missing;
    }
    if image::open(&path).unwrap().to_rgba8() == *drawn { Verdict::Matches } else { Verdict::Differs }
}

pub(crate) fn accept(name: &str, drawn: &RgbaImage) {
    std::fs::create_dir_all(dir()).unwrap();
    drawn.save(path(name)).unwrap();
}

/// Images with no screen to draw them, so nothing checks them any more.
pub(crate) fn without_a_screen(screens: &[&str]) -> Vec<String> {
    let images = std::fs::read_dir(dir()).into_iter().flatten().map(|entry| entry.unwrap().path());
    images
        .filter(|path| path.extension().is_some_and(|e| e == "png"))
        .filter_map(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .filter(|stem| !screens.contains(&stem.as_str()))
        .collect()
}

/// Screens that `screens.md` doesn't show, so their image isn't where anyone reads the spec.
pub(crate) fn not_in_the_spec<'a>(screens: &[&'a str]) -> Vec<&'a str> {
    let spec = std::fs::read_to_string(dir().with_extension("md")).unwrap_or_default();
    screens.iter().copied().filter(|name| !spec.contains(&format!("](screens/{name}.png)"))).collect()
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/screens")
}

fn path(name: &str) -> PathBuf {
    dir().join(format!("{name}.png"))
}
