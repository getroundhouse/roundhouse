//! Hashing what [`crate::cut`] kept of an upstream release, without notices, so [`crate::verify`] can check the copy
//! offline in milliseconds.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ring::digest::{Context, SHA256};

use crate::error::{At, Error};
use crate::files::{CopyPath, files_under};
use crate::notice::without_notices;
use crate::record::Tree;

/// The digest of every file and link in `dir`, with any notice lines stripped.
pub(crate) fn tree_of_dir(dir: &Path) -> Result<Tree, Error> {
    let files = files_under(dir)?;
    let sources: BTreeMap<&CopyPath, &Path> = files.iter().map(|path| (path, dir)).collect();
    tree_of_sources(&sources)
}

/// The digest of `sources`, mapping each sorted [`CopyPath`] to the folder that holds its base version.
pub(crate) fn tree_of_sources(sources: &BTreeMap<&CopyPath, &Path>) -> Result<Tree, Error> {
    let mut context = Context::new(&SHA256);
    for (path, folder) in sources {
        let file = path.under(folder);
        context.update(path.as_str().as_bytes());
        context.update(&[0]);
        if fs::symlink_metadata(&file).at(&file)?.is_symlink() {
            let target = fs::read_link(&file).at(&file)?.to_string_lossy().replace('\\', "/");
            let bytes = target.as_bytes();
            context.update(b"l");
            context.update(&(bytes.len() as u64).to_le_bytes());
            context.update(bytes);
        } else {
            let raw = fs::read(&file).at(&file)?;
            let bytes = without_notices(&raw);
            let clean = bytes.as_deref().unwrap_or(&raw);
            context.update(b"f");
            context.update(&(clean.len() as u64).to_le_bytes());
            context.update(clean);
        }
    }
    let digest = context.finish();
    let mut hex = String::with_capacity(64);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(Tree::new(hex))
}
