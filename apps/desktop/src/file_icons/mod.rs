//! The icon beside a file's name, which says what kind it is at a glance, as editors show it, and the plain folder
//! beside a folder's. They're the MIT-licensed Catppuccin icons, drawn in thin lines in its Mocha flavour's colours,
//! and each name gets the icon Catppuccin's manifest gives it, so they match an editor that uses them.
//! `tools/file-icons/update.py` puts a pinned release of them in `file-icons/`, with their license and colours, and
//! `build/file_icons.rs` fills the colours in and turns the manifest into the tables here. A file's is drawn as a
//! picture, in its own colours, and the plain file, for a file with no kind of its own, and every folder's, whatever
//! folder it is, as an icon in the muted text colour.

use gpui_kit::component::Icon;
use gpui_kit::{AnyElement, App, IntoElement as _, Styled as _, img, px};

include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));

#[cfg(test)]
mod tests;

/// The icon at `path`, if it's one of these, for the app's asset source.
pub(crate) fn load(path: &str) -> Option<&'static [u8]> {
    find(ICONS, path.strip_prefix("file-icons/")?.strip_suffix(".svg")?)
}

/// The icon for the file at `path`: its kind's, in its own colours, or the plain file, if it has no kind of its own.
pub(crate) fn file_icon(path: &str, cx: &App) -> AnyElement {
    match file_kind(path) {
        FILE => plain(FILE, cx).into_any_element(),
        kind => img(asset(kind)).size(px(ICON_SIZE)).flex_none().into_any_element(),
    }
}

/// The plain folder, open or closed.
pub(crate) fn folder_icon(open: bool, cx: &App) -> Icon {
    plain(if open { OPEN_FOLDER } else { FOLDER }, cx)
}

/// An icon that stands for no kind in particular, the plain file or a folder, in the muted text colour, so it gives way
/// to those that say what a file is. It's drawn as the kit's icons are, which keeps only its shape.
fn plain(name: &str, cx: &App) -> Icon {
    Icon::empty().path(asset(name)).size(px(ICON_SIZE)).flex_none().text_color(look::colors(cx).text_muted)
}

fn asset(name: &str) -> String {
    format!("file-icons/{name}.svg")
}

/// Which icon stands for the file at `path`, as the manifest says: by its name, then by its longest extension, then
/// by the few patterns editors know a kind of file by, and otherwise the plain file.
fn file_kind(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    find(FILE_NAMES, &name)
        .or_else(|| extensions(&name).find_map(|extension| find(FILE_EXTENSIONS, extension)))
        .or_else(|| by_pattern(&name))
        .unwrap_or(FILE)
}

/// The icon for a name editors know by a pattern rather than by a name or an extension the manifest lists: a
/// Dockerfile for one stage, such as `Dockerfile.api`, gets the one the manifest gives the plain one, and a log that's
/// been rotated, such as `app.log.1`, the one it gives logs.
fn by_pattern(name: &str) -> Option<&'static str> {
    for plain in ["dockerfile", "containerfile"] {
        if name.strip_prefix(plain).is_some_and(|rest| rest.starts_with('.')) {
            return find(FILE_NAMES, plain);
        }
    }
    let rotated = name.rsplit_once('.').is_some_and(|(rest, last)| {
        last.chars().count() == 1 && rest.rsplit_once('.').is_some_and(|(_, extension)| extension == "log")
    });
    rotated.then(|| find(FILE_EXTENSIONS, "log")).flatten()
}

/// A name's extensions, longest first, as `spec.ts` and then `ts` for `app.spec.ts`. A leading dot starts one too, so
/// `.env` has `env`.
fn extensions(name: &str) -> impl Iterator<Item = &str> {
    name.match_indices('.').map(move |(at, _)| &name[at + 1..])
}

/// What a table sorted by name gives `name`.
fn find<T: Copy>(table: &[(&str, T)], name: &str) -> Option<T> {
    table.binary_search_by_key(&name, |(each, _)| *each).ok().map(|at| table[at].1)
}
