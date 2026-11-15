//! The files of the copy, or of one of upstream's releases, by their paths from its root, and how they're copied from
//! one folder to another.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::{At, Error};
use crate::link::{link, unlink};

/// A path from the root of the copy, which is the root of upstream's repository, with `/` between its parts, as git
/// writes one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct CopyPath(String);

impl CopyPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Where it is in `dir`.
    pub(crate) fn under(&self, dir: &Path) -> PathBuf {
        dir.join(&self.0)
    }

    /// Whether it's `folder`, or something in it.
    pub(crate) fn within(&self, folder: &CopyPath) -> bool {
        self.0.strip_prefix(&folder.0).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }

    fn from_relative(path: &Path) -> Self {
        let parts: Vec<_> = (path.components())
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_string_lossy()),
                Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir => None,
            })
            .collect();
        Self(parts.join("/"))
    }
}

impl fmt::Display for CopyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Every file and link in `dir`, sorted, but a git repository's own. A link to a folder is a link, and what's in it
/// isn't listed.
pub(crate) fn files_under(dir: &Path) -> Result<Vec<CopyPath>, Error> {
    let mut found = Vec::new();
    add_files(dir, dir, &mut found)?;
    found.sort();
    Ok(found)
}

fn add_files(top: &Path, dir: &Path, found: &mut Vec<CopyPath>) -> Result<(), Error> {
    for entry in fs::read_dir(dir).at(dir)? {
        let entry = entry.at(dir)?;
        let path = entry.path();
        if entry.file_type().at(&path)?.is_dir() {
            if dir != top || entry.file_name() != ".git" {
                add_files(top, &path, found)?;
            }
        } else {
            found.push(CopyPath::from_relative(path.strip_prefix(top).unwrap_or(&path)));
        }
    }
    Ok(())
}

/// Copies each of `files` from `source` into `into`, folders and all. A link is copied as a link, and what was at each
/// place goes first, so neither a link nor a file is written through a link that was there.
pub(crate) fn put(source: &Path, files: &[CopyPath], into: &Path) -> Result<(), Error> {
    for path in files {
        let (from, to) = (path.under(source), path.under(into));
        if let Some(folder) = to.parent() {
            fs::create_dir_all(folder).at(folder)?;
        }
        if fs::symlink_metadata(&to).is_ok_and(|there| !there.is_dir()) {
            unlink(&to).at(&to)?;
        }
        if fs::symlink_metadata(&from).at(&from)?.is_symlink() {
            link(&fs::read_link(&from).at(&from)?, &to).at(&to)?;
        } else {
            fs::copy(&from, &to).at(&to)?;
        }
    }
    Ok(())
}

/// Takes `path` out of `dir`, and then each folder it leaves empty.
pub(crate) fn remove(dir: &Path, path: &CopyPath) -> Result<(), Error> {
    let gone = path.under(dir);
    unlink(&gone).at(&gone)?;
    for folder in gone.ancestors().skip(1).take_while(|folder| *folder != dir) {
        if fs::read_dir(folder).at(folder)?.next().is_some() {
            break;
        }
        fs::remove_dir(folder).at(folder)?;
    }
    Ok(())
}

/// Whether `path` is a file that still has a conflict as git marks one.
pub(crate) fn marked(path: &Path) -> Result<bool, Error> {
    if !fs::symlink_metadata(path).is_ok_and(|there| there.is_file()) {
        return Ok(false);
    }
    let text = fs::read(path).at(path)?;
    Ok(text.starts_with(b"<<<<<<< ") || text.windows(9).any(|start| start == b"\n<<<<<<< "))
}

/// Whether there's a file, a link or a folder at `path`. A link counts even when what it points to isn't there.
pub(crate) fn there(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// What `path` would be with each `..` taken back, without asking the file system, which a path that isn't there
/// can't be asked about.
pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut plain = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                plain.pop();
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => plain.push(part),
        }
    }
    plain
}

/// Reads a file that isn't there as nothing, rather than as an error.
pub(crate) fn read_if_there(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.to_path_buf(), source }),
    }
}
