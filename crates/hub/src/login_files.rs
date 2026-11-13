//! Where the hub keeps the token of each login it holds: a file for each connection, named for it, in the logins
//! directory, which `crate::logins` keeps in step with the accounts' rows. Only this user can open them, as the
//! database. A file's name is one the hub made, and one read back from the database is checked as it becomes a path,
//! so a row can't name a file anywhere else.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;

use protocol::model::LoginToken;

use crate::private;

/// The name of the file a login's token is in, which the hub makes as the login is connected and keeps on the
/// account's row: a UUID, lowercase and with its hyphens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LoginFile(String);

impl LoginFile {
    pub(crate) fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// `None` for anything but a name the hub makes, so nothing read back can climb out of the directory.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        let uuid = uuid::Uuid::try_parse(name).ok()?;
        (uuid.hyphenated().to_string() == name).then(|| Self(name.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct LoginFiles {
    dir: PathBuf,
}

impl LoginFiles {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Writes `token` as `file`, which is new, and has it last through a power cut before this returns, so a row that
    /// names it never names a file that isn't there.
    pub(crate) fn write(&self, file: &LoginFile, token: &LoginToken) -> io::Result<()> {
        private::create_dir_lasting(&self.dir)?;
        let mut handle = private::file_options().write(true).create_new(true).open(self.dir.join(file.as_str()))?;
        handle.write_all(token.as_str().as_bytes())?;
        handle.sync_all()?;
        private::sync_dir(&self.dir)
    }

    /// The token in `file`. `None` when it's gone, empty, as one a hub stopped part way through writing can be, or not
    /// text, which no token the hub wrote is.
    pub(crate) fn read(&self, file: &LoginFile) -> io::Result<Option<LoginToken>> {
        match fs::read(self.dir.join(file.as_str())) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok().filter(|token| !token.is_empty()).map(LoginToken::new)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Removes `file`, and has that last through a power cut before this returns, so a token disconnected never comes
    /// back.
    pub(crate) fn remove(&self, file: &LoginFile) -> io::Result<()> {
        match fs::remove_file(self.dir.join(file.as_str())) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            removed => removed?,
        }
        private::sync_dir(&self.dir)
    }

    /// Removes every file but those in `kept`, such as one a hub stopped before its row named it, or one of a login
    /// that was replaced.
    pub(crate) fn remove_all_but(&self, kept: &HashSet<LoginFile>) -> io::Result<()> {
        let entries = match fs::read_dir(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            entries => entries?,
        };
        let mut removed = false;
        for entry in entries {
            let entry = entry?;
            let named = entry.file_name().to_str().and_then(LoginFile::parse);
            if entry.file_type()?.is_file() && !named.is_some_and(|file| kept.contains(&file)) {
                fs::remove_file(entry.path())?;
                removed = true;
            }
        }
        if removed {
            private::sync_dir(&self.dir)?;
        }
        Ok(())
    }
}
