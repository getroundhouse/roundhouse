//! The design's canvases and films, as files under `design/`. Brilliant keeps what it makes for itself in folders that
//! start with a dot, and a workspace's own folder, which `.conductor/settings.toml` makes and git ignores, starts with
//! `@`, so neither is the design's.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// A canvas's path from the design's folder, without `.bl`, as a copy of a component names it in `canvas(…)`:
/// `Screens/Desktop/Chats/Plan to approve`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanvasPath(String);

impl CanvasPath {
    pub fn new(path: impl Into<String>) -> CanvasPath {
        CanvasPath(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Its folders and its name, from the outermost.
    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl fmt::Display for CanvasPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A file of the design and its text: a canvas by its `CanvasPath`, or a film by its path with `.bm`.
#[derive(Debug, Clone)]
pub struct File<P> {
    pub path: P,
    pub text: String,
}

/// Every canvas of the design in `design`, by path.
pub fn canvases(design: &Path) -> Result<Vec<File<CanvasPath>>, Error> {
    let found = files(design, "bl")?;
    let canvases = found.into_iter().map(|(path, text)| {
        let path = path.with_extension("");
        let parts: Vec<String> =
            path.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
        File { path: CanvasPath(parts.join("/")), text }
    });
    Ok(canvases.collect())
}

/// Every film of the design in `design`, by its path from there, such as `Screens/Motion/Opens.bm`.
pub fn films(design: &Path) -> Result<Vec<File<PathBuf>>, Error> {
    Ok(files(design, "bm")?.into_iter().map(|(path, text)| File { path, text }).collect())
}

/// Each file under `design` with `extension`, by its path from there, in order of path.
fn files(design: &Path, extension: &str) -> Result<Vec<(PathBuf, String)>, Error> {
    let mut found = Vec::new();
    walk(design, Path::new(""), extension, &mut found)?;
    found.sort();
    Ok(found)
}

/// Adds each file with `extension` in `folder`, which is at `relative` in the design, and in the folders in it.
fn walk(folder: &Path, relative: &Path, extension: &str, found: &mut Vec<(PathBuf, String)>) -> Result<(), Error> {
    for entry in fs::read_dir(folder).map_err(unreadable(folder))? {
        let entry = entry.map_err(unreadable(folder))?;
        let (path, name) = (entry.path(), entry.file_name());
        if name.to_string_lossy().starts_with(['.', '@']) {
            continue;
        }
        if path.is_dir() {
            walk(&path, &relative.join(&name), extension, found)?;
        } else if path.extension().is_some_and(|found| found == extension) {
            let text = fs::read_to_string(&path).map_err(unreadable(&path))?;
            found.push((relative.join(&name), text));
        }
    }
    Ok(())
}

/// The error for a file or folder at `path` that couldn't be read.
fn unreadable(path: &Path) -> impl Fn(io::Error) -> Error + '_ {
    move |source| Error::Read { path: path.to_path_buf(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspaces_folder_and_brilliants_own_are_not_the_designs() {
        let folder_of_the_design = tempfile::tempdir().unwrap();
        let design = folder_of_the_design.path();
        for folder in ["Screens/Chats", "@ a-workspace", ".brilliant"] {
            fs::create_dir_all(design.join(folder)).unwrap();
            fs::write(design.join(folder).join("Canvas.bl"), "bp:v1\n").unwrap();
        }

        let found: Vec<String> = canvases(design).unwrap().into_iter().map(|canvas| canvas.path.to_string()).collect();

        assert_eq!(found, ["Screens/Chats/Canvas"]);
    }
}
