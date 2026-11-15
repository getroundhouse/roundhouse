//! What a copy that keeps what a pin says holds of upstream's files at a release.

use std::fs;
use std::io;
use std::path::Path;

use toml::Table;

use crate::error::{At, Error};
use crate::files::{CopyPath, files_under, put};
use crate::members::members_cut;
use crate::notice::NOTICE;
use crate::pin::Kept;
use crate::reads::unkept;

/// Writes to `out` what a copy that keeps `kept` holds of upstream's files in `checkout`.
pub(crate) fn cut(checkout: &Path, out: &Path, kept: &[Kept]) -> Result<(), Error> {
    let (mut whole, mut left_out) = (Vec::new(), Vec::new());
    for path in kept {
        match path {
            Kept::Whole(path) => whole.push(path.clone()),
            Kept::LeftOut(path) => left_out.push(path.clone()),
        }
    }
    for path in &whole {
        let source = path.under(checkout);
        match fs::symlink_metadata(&source) {
            Ok(there) if there.is_dir() => put(&source, &files_under(&source)?, &path.under(out))?,
            Ok(_) => put(checkout, std::slice::from_ref(path), out)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Error::NotUpstream(path.clone())),
            Err(source_error) => return Err(Error::Io { path: source, source: source_error }),
        }
    }
    for path in &left_out {
        let folder = path.under(out);
        if !fs::symlink_metadata(&folder).is_ok_and(|there| there.is_dir()) {
            return Err(Error::NothingToLeaveOut(path.clone()));
        }
        fs::remove_dir_all(&folder).at(&folder)?;
    }
    let crates: Vec<CopyPath> = whole.into_iter().filter(|path| path.under(out).join("Cargo.toml").is_file()).collect();
    let manifest = out.join("Cargo.toml");
    let text = fs::read_to_string(&manifest).at(&manifest)?;
    let parsed: Table = text.parse().map_err(|source| Error::Toml { path: manifest.clone(), source })?;
    let notice = format!("# {NOTICE}\n# Its workspace lists only the packages the copy keeps.\n");
    fs::write(&manifest, notice + &members_cut(&text, &parsed, &crates, &left_out)?).at(&manifest)?;
    let unread = unkept(out)?;
    if !unread.is_empty() {
        return Err(Error::ReadsUnkept(unread));
    }
    Ok(())
}
