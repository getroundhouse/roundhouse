//! A worktree's `.context/` folder, where what isn't the repository's goes, as opposed to an agent's context, which is
//! what its model holds. Each folder of it that the worker makes holds a `.gitignore` that ignores everything in it,
//! itself included, so nothing there shows as a change, whatever the repository ignores, and nothing about the
//! repository has to change. They're deleted with the worktree when it's archived. The rest of `.context/` is left
//! alone, since a repository can keep things of its own there.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// A folder of `.context/` that the worker makes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Folder {
    /// The files a prompt carries, for the agent to open, as `crate::attachments` puts them there.
    Attachments,
    /// What an agent makes for its own use, such as notes, logs and screenshots, rather than in the system's temporary
    /// directory, where the person can't see it and an image can't show in what the agent writes. Its instructions
    /// send it here, so the worker makes it before each agent starts.
    Tmp,
}

impl Folder {
    #[cfg(test)]
    pub(crate) const ALL: [Folder; 2] = [Folder::Attachments, Folder::Tmp];

    /// Where it is, from the worktree's root.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Folder::Attachments => ".context/attachments",
            Folder::Tmp => ".context/tmp",
        }
    }

    /// This folder in the worktree at `worktree`, made if it isn't there yet, with a `.gitignore` of its own. Any
    /// folder on the way that isn't one, a link included, is refused as `NotADirectory`, or a repository could point it
    /// anywhere and have files written there. A `.gitignore` already there is kept.
    pub(crate) fn make(self, worktree: &Path) -> io::Result<PathBuf> {
        let mut dir = worktree.to_path_buf();
        for part in self.path().split('/') {
            dir.push(part);
            match std::fs::symlink_metadata(&dir) {
                Ok(found) if found.is_dir() => {}
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        format!("{} isn't a folder", dir.display()),
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => match std::fs::create_dir(&dir) {
                    Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(with_path(&e, &dir)),
                    _ => {}
                },
                Err(e) => return Err(with_path(&e, &dir)),
            }
        }
        // Never written through anything already there, a link included.
        let ignore = dir.join(".gitignore");
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&ignore) {
            Ok(mut file) => file.write_all(b"*\n").map_err(|e| with_path(&e, &ignore))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(with_path(&e, &ignore)),
        }
        Ok(dir)
    }
}

/// Keeps the error's kind, and says which path it was about, which an I/O error doesn't.
fn with_path(e: &io::Error, path: &Path) -> io::Error {
    io::Error::new(e.kind(), format!("{} couldn't be made: {e}", path.display()))
}
