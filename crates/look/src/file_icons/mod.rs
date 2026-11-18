//! The icon beside a file's name, which says what kind it is at a glance, as editors show it, and the plain folder
//! beside a folder's. They're the MIT-licensed Catppuccin icons, drawn in thin lines in its Mocha flavour's colours,
//! and each name gets the icon Catppuccin's manifest gives it, so they match an editor that uses them.
//! `tools/file-icons/update.py` puts a pinned release of them in look's `file-icons/`, with their license and colours, and
//! `build/file_icons.rs` fills the colours in and turns the manifest into the tables here. A file's is drawn as a
//! picture, in its own colours, and the plain file, for a file with no kind of its own, and every folder's, whatever
//! folder it is, as an icon in the muted text colour.

#[cfg(test)]
mod tests;

mod lookup;
pub(crate) use lookup::file_icon_at;
#[cfg(test)]
use lookup::{FILE, FILE_EXTENSIONS, FILE_NAMES, FOLDER, ICON_SIZE, ICONS, OPEN_FOLDER, asset, file_kind};
pub use lookup::{FILE_ICON, file_icon, file_icon_asset, folder_icon};
