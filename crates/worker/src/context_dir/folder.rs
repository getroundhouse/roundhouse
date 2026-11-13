use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// `.context/`, or a folder of it, that the worker makes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Folder {
    /// `.context/` itself, where an agent keeps what it makes for its own use, such as notes, logs and screenshots, in
    /// folders it names for what they hold, rather than in the system's temporary directory, where the person can't
    /// see it and an image can't show in what the agent writes. Its instructions send it here, so the worker makes it
    /// before each agent starts.
    Context,
    /// The files a prompt carries, for the agent to open, as `crate::attachments` puts them there.
    Attachments,
}

impl Folder {
    #[cfg(test)]
    pub(crate) const ALL: [Folder; 2] = [Folder::Context, Folder::Attachments];

    /// Where it is, from the worktree's root.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Folder::Context => ".context",
            Folder::Attachments => ".context/attachments",
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
