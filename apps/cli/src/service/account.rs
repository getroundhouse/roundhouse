//! The account a hub installed as a service runs as, from the system's database of users.

use std::path::PathBuf;
use std::process::Command;

/// A user of this machine, as the system's database of users has them.
pub(crate) struct Account {
    pub(crate) name: String,
    ids: Ids,
    pub(crate) home: PathBuf,
    /// Their login shell, which a workspace's terminals run.
    pub(crate) shell: Option<String>,
}

/// Who the system takes the user for, which a process is started as.
#[cfg(unix)]
struct Ids {
    uid: u32,
    gid: u32,
}

/// None, since Windows runs no hub as a service yet, so no account is found there.
#[cfg(windows)]
enum Ids {}

#[cfg(unix)]
impl Account {
    /// The user called `name`.
    pub(crate) fn named(name: &str) -> anyhow::Result<Self> {
        use std::ffi::{CStr, CString, OsStr};
        use std::os::unix::ffi::OsStrExt as _;

        let c_name = CString::new(name)?;
        // SAFETY: `passwd` is a struct of integers and pointers, for which all zeros is a valid value.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let mut buffer: Vec<libc::c_char> = vec![0; 4096];
        loop {
            // SAFETY: every pointer is to memory this function owns for the call, `buffer`'s length is what's passed,
            // and the strings `entry` points to are in `buffer`, which outlives their use below.
            let error = unsafe {
                libc::getpwnam_r(c_name.as_ptr(), &raw mut entry, buffer.as_mut_ptr(), buffer.len(), &raw mut found)
            };
            match error {
                0 => break,
                libc::ERANGE => buffer.resize(buffer.len() * 2, 0),
                error => return Err(std::io::Error::from_raw_os_error(error).into()),
            }
        }
        anyhow::ensure!(!found.is_null(), "this machine has no user called {name}");
        // SAFETY: `found` is `entry`, whose strings getpwnam_r set to null or to strings ending in `buffer`.
        let text = |pointer: *const libc::c_char| unsafe {
            (!pointer.is_null()).then(|| OsStr::from_bytes(CStr::from_ptr(pointer).to_bytes()).to_owned())
        };
        let home = text(entry.pw_dir).filter(|home| !home.is_empty()).map(PathBuf::from);
        let home = home.ok_or_else(|| anyhow::anyhow!("{name} has no home directory"))?;
        let shell = text(entry.pw_shell).and_then(|shell| shell.into_string().ok()).filter(|shell| !shell.is_empty());
        Ok(Self { name: name.to_owned(), ids: Ids { uid: entry.pw_uid, gid: entry.pw_gid }, home, shell })
    }

    /// Has `command` run as this user, rather than as this process's.
    pub(crate) fn runs(&self, command: &mut Command) {
        use std::os::unix::process::CommandExt as _;

        command.uid(self.ids.uid).gid(self.ids.gid);
    }
}

#[cfg(windows)]
impl Account {
    /// Refused, since a hub can't be installed as a service on Windows yet, so no account is needed for one.
    pub(crate) fn named(_name: &str) -> anyhow::Result<Self> {
        anyhow::bail!("installing a hub as a service isn't built on Windows yet")
    }

    /// Never called, since no account is found on Windows.
    pub(crate) fn runs(&self, _command: &mut Command) {
        match self.ids {}
    }
}

/// Whether this process can register a service, which takes an administrator.
#[cfg(unix)]
pub(crate) fn is_administrator() -> bool {
    // SAFETY: geteuid takes nothing and can't fail.
    unsafe { libc::geteuid() == 0 }
}

/// Never, since Windows registers no service yet, and [`Account::named`] says so first.
#[cfg(windows)]
pub(crate) fn is_administrator() -> bool {
    false
}
