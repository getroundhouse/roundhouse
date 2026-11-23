//! The user of this machine a hub installed as a service runs as, from the system's database of users.

use std::path::PathBuf;
use std::process::Command;

/// A user of this machine, as the system's database of users has them.
pub(crate) struct ServiceUser {
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

/// None, since Windows runs no hub as a service yet, so no user is found there.
#[cfg(windows)]
enum Ids {}

#[cfg(unix)]
impl ServiceUser {
    /// The user called `name`.
    pub(crate) fn named(name: &str) -> anyhow::Result<Self> {
        let found = process::OsUser::named(name)?;
        let user = found.ok_or_else(|| anyhow::anyhow!("this machine has no user called {name}"))?;
        let home = user.home.ok_or_else(|| anyhow::anyhow!("{name} has no home directory"))?;
        let ids = Ids { uid: user.uid, gid: user.gid };
        Ok(Self { name: name.to_owned(), ids, home, shell: user.shell })
    }

    /// Has `command` run as this user, rather than as this process's.
    pub(crate) fn runs(&self, command: &mut Command) {
        use std::os::unix::process::CommandExt as _;

        command.uid(self.ids.uid).gid(self.ids.gid);
    }
}

#[cfg(windows)]
impl ServiceUser {
    /// Refused, since a hub can't be installed as a service on Windows yet, so no user is needed for one.
    pub(crate) fn named(_name: &str) -> anyhow::Result<Self> {
        anyhow::bail!("installing a hub as a service isn't built on Windows yet")
    }

    /// Never called, since no user is found on Windows.
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

/// Never, since Windows registers no service yet, and [`ServiceUser::named`] says so first.
#[cfg(windows)]
pub(crate) fn is_administrator() -> bool {
    false
}
