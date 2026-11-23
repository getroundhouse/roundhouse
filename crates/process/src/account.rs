//! A user of this machine, as the system's database of users has them.

#[cfg(unix)]
use std::ffi::{CStr, CString, OsStr};
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt as _;
#[cfg(unix)]
use std::path::PathBuf;

/// A user of this machine.
#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    /// Their home directory, unless it's empty.
    pub home: Option<PathBuf>,
    /// Their login shell, unless it's empty or isn't text.
    pub shell: Option<String>,
}

/// Which user to look up.
#[cfg(unix)]
enum Key {
    Name(CString),
    Id(libc::uid_t),
}

#[cfg(unix)]
impl Account {
    /// The user called `name`, or `None` if this machine has none.
    pub fn named(name: &str) -> io::Result<Option<Self>> {
        let name = CString::new(name).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        look_up(&Key::Name(name))
    }

    /// The user this process runs as, or `None` if the system's database has no entry for it, as a container's can
    /// lack.
    pub fn of_this_process() -> io::Result<Option<Self>> {
        // SAFETY: getuid takes nothing and can't fail.
        let uid = unsafe { libc::getuid() };
        look_up(&Key::Id(uid))
    }
}

/// The login shell of the user this process runs as, which a hub asks for the environment the user's profile gives
/// when it wasn't told one.
#[cfg(unix)]
pub fn account_shell() -> Option<String> {
    Account::of_this_process().ok().flatten().and_then(|account| account.shell)
}

/// None, since Windows gives an account no login shell.
#[cfg(windows)]
pub fn account_shell() -> Option<String> {
    None
}

/// With `getpwnam_r` or `getpwuid_r`, which unlike `getpwnam` and `getpwuid` write to memory of the caller's, so
/// another thread looking someone up can't overwrite what's read.
#[cfg(unix)]
fn look_up(key: &Key) -> io::Result<Option<Account>> {
    // SAFETY: `passwd` is a struct of integers and pointers, for which all zeros is a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let mut buffer: Vec<libc::c_char> = vec![0; 4096];
    loop {
        let (into, length) = (buffer.as_mut_ptr(), buffer.len());
        // SAFETY: every pointer is to memory this function owns for the call, `buffer`'s length is what's passed, and
        // the strings `entry` points to are in `buffer`, which outlives their use below.
        let error = unsafe {
            match key {
                Key::Name(name) => libc::getpwnam_r(name.as_ptr(), &raw mut entry, into, length, &raw mut found),
                Key::Id(uid) => libc::getpwuid_r(*uid, &raw mut entry, into, length, &raw mut found),
            }
        };
        match error {
            0 => break,
            libc::ERANGE => buffer.resize(buffer.len() * 2, 0),
            error => return Err(io::Error::from_raw_os_error(error)),
        }
    }
    if found.is_null() {
        return Ok(None);
    }
    // SAFETY: `found` is `entry`, whose strings the lookup set to null or to strings ending in `buffer`.
    let text = |pointer: *const libc::c_char| unsafe {
        (!pointer.is_null()).then(|| OsStr::from_bytes(CStr::from_ptr(pointer).to_bytes()).to_owned())
    };
    let name = text(entry.pw_name).and_then(|name| name.into_string().ok()).unwrap_or_default();
    let home = text(entry.pw_dir).filter(|home| !home.is_empty()).map(PathBuf::from);
    let shell = text(entry.pw_shell).and_then(|shell| shell.into_string().ok()).filter(|shell| !shell.is_empty());
    Ok(Some(Account { name, uid: entry.pw_uid, gid: entry.pw_gid, home, shell }))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// Looked up by its ID and then by the name that gave, the user running the tests is the same both ways.
    #[test]
    fn this_processs_user_is_found_by_its_id_and_its_name() {
        let by_id = Account::of_this_process().unwrap().expect("the user running the tests has an entry");

        let by_name = Account::named(&by_id.name).unwrap();

        // SAFETY: getuid takes nothing and can't fail.
        assert_eq!(by_id.uid, unsafe { libc::getuid() });
        assert_eq!(by_name, Some(by_id));
        assert_eq!(Account::named("no-such-user-here").unwrap(), None);
    }
}
