//! The pictures the guide's pages show: PNGs in the repository, mostly the app's own screens from
//! `docs/spec/screens`, published beside the pages.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{Error, io_error};
use crate::page::PagePath;
use crate::source::{RepoPath, Source};

/// The directory the pictures are published in, from the guide's own. Each is at its path in the repository under it,
/// so two of one name in different directories are two files.
const DIR: &str = "images";

/// The gallery draws the app's screens at two pixels to a point, so a picture is shown at half its pixels, and is
/// sharp on a display with two pixels to a point.
const PIXELS_PER_POINT: u32 = 2;

/// A picture as a page shows it.
pub(crate) struct Picture {
    /// Where it's published, from the guide's own directory.
    pub(crate) address: String,
    pub(crate) width_points: u32,
    pub(crate) height_points: u32,
}

/// Every picture some page shows.
#[derive(Default)]
pub(crate) struct Pictures {
    shown: BTreeSet<RepoPath>,
}

impl Pictures {
    /// Notes that `page` shows the PNG at `file`, and says how to show it.
    pub(crate) fn show(&mut self, source: &Source, page: &PagePath, file: &RepoPath) -> Result<Picture, Error> {
        let Some((width, height)) = png_size(&source.on_disk(file))? else {
            return Err(Error::NotAPng { page: page.clone(), picture: file.clone() });
        };
        self.shown.insert(file.clone());
        Ok(Picture {
            address: format!("{DIR}/{file}"),
            width_points: width / PIXELS_PER_POINT,
            height_points: height / PIXELS_PER_POINT,
        })
    }

    /// Copies each picture into the guide as it's built in `out`, and says how many there were.
    pub(crate) fn publish(&self, source: &Source, out: &Path) -> Result<usize, Error> {
        for file in &self.shown {
            let from = source.on_disk(file);
            let to = out.join(DIR).join(file.as_str());
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir).map_err(io_error(dir))?;
            }
            fs::copy(&from, &to).map_err(io_error(&from))?;
        }
        Ok(self.shown.len())
    }
}

/// A PNG's width and height in pixels, from its header, or `None` if the file doesn't start as a PNG does.
fn png_size(path: &Path) -> Result<Option<(u32, u32)>, Error> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    // The signature, then the first chunk's length and its name, `IHDR`, then the width and the height.
    let mut header = [0u8; 24];
    let read = fs::File::open(path).and_then(|mut file| file.read(&mut header)).map_err(io_error(path))?;
    if read < header.len() || !header.starts_with(SIGNATURE) || &header[12..16] != b"IHDR" {
        return Ok(None);
    }
    let number = |at: usize| u32::from_be_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
    Ok(Some((number(16), number(20))))
}
