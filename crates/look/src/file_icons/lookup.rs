use gpui_kit::component::Icon;
use gpui_kit::{AnyElement, App, IntoElement as _, Styled as _, img};

use crate::colors::colors;
use crate::icon_sizes::{IconSize, IconSizing as _};

include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));

/// How big a file's or a folder's icon is drawn: a large icon, since it stands for its row. Each is embedded at this
/// size, as `build/file_icons.rs` has it, which a test holds to it.
pub const FILE_ICON: IconSize = IconSize::Large;

/// The icon at `path`, if it's one of these, for the app's asset source.
pub fn file_icon_asset(path: &str) -> Option<&'static [u8]> {
    find(ICONS, path.strip_prefix("file-icons/")?.strip_suffix(".svg")?)
}

/// The icon for the file at `path`: its kind's, in its own colours, or the plain file, if it has no kind of its own.
pub fn file_icon(path: &str, cx: &App) -> AnyElement {
    file_icon_at(path, FILE_ICON, cx)
}

/// The icon for the file at `path`, as [`file_icon`] draws it, at `size`, such as a chip's.
pub(crate) fn file_icon_at(path: &str, size: IconSize, cx: &App) -> AnyElement {
    match file_kind(path) {
        FILE => plain(FILE, cx).sized(size).into_any_element(),
        kind => img(asset(kind)).size(size.size()).flex_none().into_any_element(),
    }
}

/// The plain folder, open or closed.
pub fn folder_icon(open: bool, cx: &App) -> Icon {
    plain(if open { OPEN_FOLDER } else { FOLDER }, cx)
}

/// An icon that stands for no kind in particular, the plain file or a folder, in the muted text colour, so it gives way
/// to those that say what a file is. It's drawn as the kit's icons are, which keeps only its shape.
fn plain(name: &str, cx: &App) -> Icon {
    Icon::empty().path(asset(name)).sized(FILE_ICON).flex_none().text_color(colors(cx).text_muted)
}

pub(super) fn asset(name: &str) -> String {
    format!("file-icons/{name}.svg")
}

/// Which icon stands for the file at `path`, as the manifest says: by its name, then by its longest extension, then
/// by the few patterns editors know a kind of file by, and otherwise the plain file.
pub(super) fn file_kind(path: &str) -> &'static str {
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
