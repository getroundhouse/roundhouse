//! Our patches to the copy, kept one to a file in [`PATCHES`], in the order [`crate::Record`] lists them.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};

use crate::error::{At, Error};
use crate::files::CopyPath;

/// Where the patch files live, from the root of this repository.
pub const PATCHES: &str = "third_party/gpui-kit/patches";

/// A patch's name, without `.patch`, such as `code-block-actions`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PatchName(String);

impl PatchName {
    /// Parses `input`, accepting an optional `.patch` suffix.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let bare = input.strip_suffix(".patch").unwrap_or(input);
        Self::parse_bare(bare).map_err(|()| Error::InvalidPatchName(input.to_string()))
    }

    fn parse_bare(bare: &str) -> Result<Self, ()> {
        let valid = !bare.is_empty()
            && !bare.starts_with('-')
            && bare.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid.then(|| Self(bare.to_string())).ok_or(())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Its file's name in [`PATCHES`].
    pub fn file_name(&self) -> String {
        format!("{}.patch", self.0)
    }

    /// Where its file is in the repository at `root`.
    pub(crate) fn path(&self, root: &Path) -> PathBuf {
        root.join(PATCHES).join(self.file_name())
    }
}

impl fmt::Display for PatchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PatchName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse_bare(&raw).map_err(|()| serde::de::Error::custom(Error::InvalidPatchName(raw)))
    }
}

/// Checks that `listed` has no duplicates and matches the `.patch` files in [`PATCHES`] at `root`.
pub(crate) fn check_patch_files(root: &Path, listed: &[PatchName]) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for name in listed {
        if !seen.insert(name) {
            return Err(Error::DuplicatePatch(name.clone()));
        }
        let path = name.path(root);
        let present = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() > 0);
        if !present {
            return Err(Error::MissingPatch(name.clone()));
        }
    }
    let expected: BTreeSet<String> = listed.iter().map(PatchName::file_name).collect();
    let dir = root.join(PATCHES);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(Error::Io { path: dir, source }),
    };
    let mut unlisted = Vec::new();
    for entry in entries {
        let name = entry.at(&dir)?.file_name().to_string_lossy().into_owned();
        if !expected.contains(&name) {
            unlisted.push(name);
        }
    }
    unlisted.sort();
    if let Some(first) = unlisted.into_iter().next() {
        return Err(Error::UnlistedPatch(first));
    }
    Ok(())
}

/// Every path in the copy that any of `listed` reads or changes.
pub(crate) fn touched_by_patches(root: &Path, listed: &[PatchName]) -> Result<BTreeSet<CopyPath>, Error> {
    let mut touched = BTreeSet::new();
    for name in listed {
        let path = name.path(root);
        let raw = fs::read(&path).at(&path)?;
        for line in String::from_utf8_lossy(&raw).lines() {
            if let Some(rest) = line.strip_prefix("diff --git a/")
                && let Some((from, to)) = rest.split_once(" b/")
            {
                touched.insert(CopyPath::new(from));
                touched.insert(CopyPath::new(to));
            } else if let Some(file) = line
                .strip_prefix("--- a/")
                .or_else(|| line.strip_prefix("+++ b/"))
                .or_else(|| line.strip_prefix("rename from "))
                .or_else(|| line.strip_prefix("rename to "))
            {
                touched.insert(CopyPath::new(file));
            }
        }
    }
    Ok(touched)
}

/// Writes `diff` to `name`'s file in [`PATCHES`] at `root`.
pub(crate) fn write_patch(root: &Path, name: &PatchName, diff: &str) -> Result<(), Error> {
    let dir = root.join(PATCHES);
    fs::create_dir_all(&dir).at(&dir)?;
    let path = name.path(root);
    fs::write(&path, diff).at(&path)
}

/// Deletes `name`'s file in [`PATCHES`] at `root`, and the folder itself when that leaves it empty.
pub(crate) fn remove_patch(root: &Path, name: &PatchName) -> Result<(), Error> {
    let path = name.path(root);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => return Err(Error::Io { path, source }),
    }
    let dir = root.join(PATCHES);
    if fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_none()) {
        fs::remove_dir(&dir).at(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_names_accept_optional_extension_and_refuse_paths() {
        assert_eq!(PatchName::parse("strong-style").unwrap().as_str(), "strong-style");
        assert_eq!(PatchName::parse("strong_style_2.patch").unwrap().as_str(), "strong_style_2");
        for bad in ["", ".patch", "-flag", "../escape", "a/b", "with space", "dot.inside"] {
            assert!(PatchName::parse(bad).is_err(), "{bad}");
        }
    }
}
