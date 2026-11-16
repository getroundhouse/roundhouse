use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use parking_lot::{Mutex, MutexGuard};
use protocol::client::ApiError;
use tracing::{info, warn};

use super::{LoginFile, LoginSecret, Stored};
use crate::error::internal;
use crate::lasting;

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
    /// What `file` holds, as [`Stored`] says.
    pub(crate) fn read(&self, file: &LoginFile) -> io::Result<Stored<S>> {
        match fs::read(self.path(file)) {
            Ok(bytes) if bytes.is_empty() => Ok(Stored::Nothing),
            Ok(bytes) => Ok(S::from_bytes(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Stored::Nothing),
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
                Ok(Stored::Secret(read)) if read == *secret => return Some(id),
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

    /// Replaces what `file` holds with `secret`, keeping its name, so the row that names it needn't change: a file
    /// beside it, which lasts through a power cut, renamed over it, and then the rename made to last too. So whatever
    /// stops the hub, or the machine, leaves `file` holding the old secret or the new, whole, and never the new in a
    /// file no row names, which tidying would remove. For a secret that takes the place of the only one that works, as
    /// a sign-in's renewed tokens do, since GitHub revokes the old as it gives the new, where [`Locked::connect`] is
    /// for one beside which the old still works. What a hub stopped before renaming leaves beside it is no login's
    /// file, so tidying removes it, and so does the next replace, before it makes its own, which it makes new, so the
    /// secret is never written into a file it didn't make, whose mode or link it would keep.
    pub(crate) fn replace(&self, file: &LoginFile, secret: &S) -> io::Result<()> {
        let beside = self.dir().join(format!("{}.partial", file.as_str()));
        match fs::remove_file(&beside) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            removed => removed?,
        }
        let handle = private_files::file_options().write(true).create_new(true).open(&beside)?;
        write_lasting(handle, secret)?;
        fs::rename(&beside, self.path(file))?;
        lasting::sync_dir(self.dir())
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
    /// as one naming a file the hub doesn't make. Each row whose file holds nothing, as [`Stored::Nothing`] says, or
    /// that doesn't hold the one it names, is given to `clear_row`, and then every file but the other rows' is removed,
    /// so a row is cleared even when a file can't be. A file in a form only a newer build reads is left with its row,
    /// for that build, and so is one that can't be read, and both are said, so the rest are still tidied.
    pub(crate) fn tidy<Id: fmt::Display>(
        &self,
        named: Vec<(Id, Option<LoginFile>)>,
        mut clear_row: impl FnMut(&Id) -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        let dir = self.dir().display();
        let mut kept = HashSet::new();
        for (id, file) in named {
            let held = match &file {
                Some(file) => self.read(file),
                None => Ok(Stored::Nothing),
            };
            match (file, held) {
                (Some(file), Ok(Stored::Secret(_))) => {
                    kept.insert(file);
                }
                (Some(file), Ok(Stored::Newer)) => {
                    info!(account = %id, %dir, "kept the hub's login for an account, in a form a newer build wrote");
                    kept.insert(file);
                }
                (Some(file), Err(e)) => {
                    warn!(account = %id, %dir, "couldn't read the secret of the hub's login for an account: {e}");
                    kept.insert(file);
                }
                (None, _) | (Some(_), Ok(Stored::Nothing)) => {
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
        let handle = private_files::file_options().write(true).create_new(true).open(self.path(file))?;
        write_lasting(handle, secret)?;
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

/// Writes `secret` to `handle`, a file opened to write it, and has it last through a power cut before this returns.
fn write_lasting<S: LoginSecret>(mut handle: fs::File, secret: &S) -> io::Result<()> {
    handle.write_all(&secret.to_bytes()?)?;
    handle.sync_all()
}
