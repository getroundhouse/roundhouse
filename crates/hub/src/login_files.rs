//! Where the hub keeps the secret of each login it holds, such as a token: a file for each connection, named for it, in
//! a directory of the logins of one kind, which no other kind's are in, and whose names one table's rows keep. Only
//! this user can open them, as the database. A file's name is one the hub made, and one read back from the database is
//! checked as it becomes a path, so a row can't name a file anywhere else.
//!
//! The writes of a file and of the row that names it are in an order chosen so that whatever stops the hub, or the
//! machine, leaves either a file no row names or a row naming a file that's gone. As the hub starts, [`Locked::tidy`]
//! mends both. So a login the hub has said it holds is never lost, and a secret it was told to forget never comes
//! back. Only this module writes or removes a file, so no caller can get the order wrong.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use parking_lot::{Mutex, MutexGuard};
use protocol::client::ApiError;
use tracing::{info, warn};

use crate::error::internal;
use crate::lasting;

#[cfg(test)]
mod tests;

/// What a login's file holds, which is all it takes to use the login, so a type that's one keeps it out of logs, as
/// `LoginToken`'s `Debug` does by saying only that it's there.
pub(crate) trait LoginSecret: Sized {
    /// The directory its files are in, in the hub's data directory, which is no other kind's, so tidying one kind's
    /// files never removes another's.
    const DIR: &'static str;

    /// What its file holds, which isn't empty, since an empty file holds none.
    fn to_bytes(&self) -> Vec<u8>;

    /// The secret in a file's `bytes`, which are never empty, or `None` when they aren't one.
    fn from_bytes(bytes: Vec<u8>) -> Option<Self>;
}

/// The name of the file a login's secret is in, which the hub makes as the login is connected and keeps on its row: a
/// UUID, lowercase and with its hyphens.
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

/// The files of the logins whose secret is an `S`, in [`LoginSecret::DIR`]. Synchronous, and only used off the async
/// threads.
pub(crate) struct LoginFiles<S> {
    dir: PathBuf,
    /// A `parking_lot` lock held through file syncs and a synced commit of the rows that name the files, which is
    /// right only because it's only taken off the async threads, as the store's own are, and guards nothing in memory.
    one_at_a_time: Mutex<()>,
    secret: PhantomData<S>,
}

impl<S: LoginSecret> LoginFiles<S> {
    /// The files in `S`'s directory in `data_dir`, the hub's data directory.
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self { dir: data_dir.join(S::DIR), one_at_a_time: Mutex::new(()), secret: PhantomData }
    }

    /// The files, and the rows that name them, while nothing else reads or changes them, until what this returns is
    /// dropped, so no connect, disconnect or tidying finds another half done.
    pub(crate) fn lock(&self) -> Locked<'_, S> {
        Locked { files: self, _one_at_a_time: self.one_at_a_time.lock() }
    }
}

/// The files of the logins of one kind, held by [`LoginFiles::lock`].
pub(crate) struct Locked<'a, S> {
    files: &'a LoginFiles<S>,
    _one_at_a_time: MutexGuard<'a, ()>,
}

impl<S: LoginSecret> Locked<'_, S> {
    /// The secret in `file`. `None` when it's gone, empty, as one a hub stopped part way through writing can be, or
    /// not an `S`, as [`LoginSecret::from_bytes`] says.
    pub(crate) fn read(&self, file: &LoginFile) -> io::Result<Option<S>> {
        match fs::read(self.path(file)) {
            Ok(bytes) if bytes.is_empty() => Ok(None),
            Ok(bytes) => Ok(S::from_bytes(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The ID of the row, of `rows`, whose file holds `secret`, each with the file it names. A file that can't be read
    /// is said and passed over, so one row's file doesn't stop another's login being connected.
    pub(crate) fn holding<Id: fmt::Display>(
        &self,
        rows: impl IntoIterator<Item = (Id, LoginFile)>,
        secret: &S,
    ) -> Option<Id>
    where
        S: PartialEq,
    {
        let dir = self.dir().display();
        for (id, file) in rows {
            match self.read(&file) {
                Ok(read) if read.as_ref() == Some(secret) => return Some(id),
                Ok(_) => {}
                Err(e) => warn!(account = %id, %dir, "couldn't read the secret of the hub's login for an account: {e}"),
            }
        }
        None
    }

    /// Keeps `secret` in a new file, and then has `write_row` write the row that names it, returning what that
    /// returns but for the file of the login the row named before, which is then let go of. The file lasts through a
    /// power cut before the row is written, so a row never names a file that isn't there, and a hub that stops between
    /// the two leaves a file no row names. A file `write_row` fails to name is let go of, and the one it would have
    /// replaced is kept.
    pub(crate) fn connect<T>(
        &self,
        secret: &S,
        write_row: impl FnOnce(&LoginFile) -> Result<(T, Option<LoginFile>), ApiError>,
    ) -> Result<T, ApiError> {
        let file = LoginFile::new();
        self.write(&file, secret).map_err(|e| {
            self.let_go(&file);
            internal(format!("couldn't keep the login's secret: {e}"))
        })?;
        let (written, replaced) = write_row(&file).inspect_err(|_| self.let_go(&file))?;
        if let Some(replaced) = replaced {
            self.let_go(&replaced);
        }
        Ok(written)
    }

    /// Removes `file`, and then has `clear_row` clear the row that names it, returning what that returns. The file
    /// goes first, and lasts through a power cut, so nothing that stops the hub or the machine between the two brings
    /// the secret back: a row whose file is gone is cleared as the hub next starts.
    pub(crate) fn disconnect<T>(
        &self,
        file: &LoginFile,
        clear_row: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        self.remove(file).map_err(|e| internal(format!("couldn't forget the login's secret: {e}")))?;
        clear_row()
    }

    /// Makes the files and the rows of the table that names them agree, as the hub starts. `named` is each of its rows
    /// that names a file, by the row's ID, with the file, or `None` for a row that names one but doesn't hold it, such
    /// as one naming a file the hub doesn't make. Each row whose file is gone, empty or not an `S`, or that doesn't hold
    /// the one it names, is given to `clear_row`, and then every file but the other rows' is removed, so a row is
    /// cleared even when a file can't be. A file that can't be read is left with its row, and said, so the rest are
    /// still tidied.
    pub(crate) fn tidy<Id: fmt::Display>(
        &self,
        named: Vec<(Id, Option<LoginFile>)>,
        mut clear_row: impl FnMut(&Id) -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        let dir = self.dir().display();
        let mut kept = HashSet::new();
        for (id, file) in named {
            let held = match &file {
                Some(file) => self.read(file).map(|secret| secret.is_some()),
                None => Ok(false),
            };
            match (file, held) {
                (Some(file), Ok(true)) => {
                    kept.insert(file);
                }
                (Some(file), Err(e)) => {
                    warn!(account = %id, %dir, "couldn't read the secret of the hub's login for an account: {e}");
                    kept.insert(file);
                }
                (None, _) | (Some(_), Ok(false)) => {
                    info!(account = %id, %dir, "the secret of the hub's login for an account is gone, so it holds none");
                    clear_row(&id)?;
                }
            }
        }
        self.remove_all_but(&kept).map_err(|e| internal(format!("couldn't let go of the logins' files in {dir}: {e}")))
    }

    fn dir(&self) -> &Path {
        &self.files.dir
    }

    fn path(&self, file: &LoginFile) -> PathBuf {
        self.dir().join(file.as_str())
    }

    /// Writes `secret` as `file`, which is new, and has it last through a power cut before this returns.
    fn write(&self, file: &LoginFile, secret: &S) -> io::Result<()> {
        lasting::create_dir_all(self.dir())?;
        let mut handle = private_files::file_options().write(true).create_new(true).open(self.path(file))?;
        handle.write_all(&secret.to_bytes())?;
        handle.sync_all()?;
        lasting::sync_dir(self.dir())
    }

    /// Removes `file`, and has that last through a power cut before this returns.
    fn remove(&self, file: &LoginFile) -> io::Result<()> {
        match fs::remove_file(self.path(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            removed => removed?,
        }
        lasting::sync_dir(self.dir())
    }

    /// Removes a file no row names, or will. One that can't be is left for [`Locked::tidy`] as the hub next starts,
    /// since nothing reads a file no row names, so what was asked still counts.
    fn let_go(&self, file: &LoginFile) {
        if let Err(e) = self.remove(file) {
            let dir = self.dir().display();
            warn!(%dir, "couldn't let go of a login's file no account names, which goes as the hub next starts: {e}");
        }
    }

    /// Removes every file but those in `kept`, such as one a hub stopped before its row named it, or one of a login
    /// that was replaced.
    fn remove_all_but(&self, kept: &HashSet<LoginFile>) -> io::Result<()> {
        let entries = match fs::read_dir(self.dir()) {
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
            lasting::sync_dir(self.dir())?;
        }
        Ok(())
    }
}
