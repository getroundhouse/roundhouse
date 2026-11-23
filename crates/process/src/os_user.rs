//! A user of this machine, as the system's database of users has them. Not an account, which is an agent's login.

#[cfg(unix)]
use std::ffi::{CStr, CString, OsStr, OsString};
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt as _;
use std::path::PathBuf;

/// A user of this machine.
#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsUser {
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
impl OsUser {
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

/// The home directory and login shell of the user this process runs as, which a hub falls back on for whichever
/// whatever started it didn't say, as a service manager or a container may not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeAndShell {
    pub home: Option<PathBuf>,
    pub shell: Option<String>,
}

/// What the system's database of users has for the user this process runs as, with neither where it has no entry.
#[cfg(unix)]
pub fn home_and_shell() -> HomeAndShell {
    let user = OsUser::of_this_process().ok().flatten();
    user.map_or_else(HomeAndShell::default, |user| HomeAndShell { home: user.home, shell: user.shell })
}

/// Neither, since Windows gives a user no login shell, and a hub there has its home from its environment, as
/// Windows gives every process.
#[cfg(windows)]
pub fn home_and_shell() -> HomeAndShell {
    HomeAndShell::default()
}

/// With `getpwnam_r` or `getpwuid_r`, which unlike `getpwnam` and `getpwuid` write to memory of the caller's, so
/// another thread looking someone up can't overwrite what's read.
#[cfg(unix)]
fn look_up(key: &Key) -> io::Result<Option<OsUser>> {
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
    // SAFETY: `found` is `entry`, whose strings the lookup set to null or to strings ending in `buffer`, which lives
    // until this function returns.
    let (name, home, shell) = unsafe { (text(entry.pw_name), text(entry.pw_dir), text(entry.pw_shell)) };
    let name = name.and_then(|name| name.into_string().ok()).unwrap_or_default();
    let home = home.filter(|home| !home.is_empty()).map(PathBuf::from);
    let shell = shell.and_then(|shell| shell.into_string().ok()).filter(|shell| !shell.is_empty());
    Ok(Some(OsUser { name, uid: entry.pw_uid, gid: entry.pw_gid, home, shell }))
}

/// The string `pointer` points to, or `None` if it's null.
///
/// # Safety
///
/// `pointer` is null, or points to a string ending in NUL that lives at least as long as the call.
#[cfg(unix)]
unsafe fn text(pointer: *const libc::c_char) -> Option<OsString> {
    // SAFETY: it's not null, so it's a string ending in NUL that lives through this, as the caller promises.
    (!pointer.is_null()).then(|| OsStr::from_bytes(unsafe { CStr::from_ptr(pointer) }.to_bytes()).to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// Looked up by its ID and then by the name that gave, the user running the tests is the same both ways.
    #[test]
    fn this_processs_user_is_found_by_its_id_and_its_name() {
        let by_id = OsUser::of_this_process().unwrap().expect("the user running the tests has an entry");

        let by_name = OsUser::named(&by_id.name).unwrap();

        // SAFETY: getuid takes nothing and can't fail.
        assert_eq!(by_id.uid, unsafe { libc::getuid() });
        assert_eq!(by_name, Some(by_id));
        assert_eq!(OsUser::named("no-such-user-here").unwrap(), None);
    }
}
