//! What the hub keeps of the environment it was started with. Nothing else of it reaches what the hub starts, such as
//! what a Claude session, tmux or a Python virtualenv set in the terminal the hub was started from.

use std::ffi::OsString;

/// Variables, each a name and its value, in the order they're given, so a later one goes in place of an earlier one of
/// the same name.
pub(super) type Vars = Vec<(String, String)>;

/// The variables the OS session sets and a profile doesn't, by name: who the user is, their home, shell and temporary
/// files, their SSH agent, and on Linux their desktop session. On Windows they're what any process there needs to start,
/// and the user's own folders, which a user's environment block has too, kept for when it can't be made.
const SESSION: &[&str] = if cfg!(windows) {
    &["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "USERNAME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP"]
} else {
    &[
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "TMPDIR",
        "LANG",
        "LANGUAGE",
        "SSH_AUTH_SOCK",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
    ]
};

/// The start of each of the locale's variables but [`SESSION`]'s, such as `LC_ALL` and `LC_CTYPE`.
const LOCALE: &str = "LC_";

/// The start of the hub's own variables, which whoever started it set for it on purpose, such as where an agent's CLI
/// is, and which its workers read too.
const OWN: &str = "ROUNDHOUSE_";

/// The start of a stand-in's variables, which a test gives a hub for a stand-in it starts in place of a program, such
/// as the fake agent's `FAKE_CLAUDE_STATE`. Kept only by a build with `testing`, as only that build takes a test's
/// timers, and they reach what a workspace runs, unlike the hub's own, which a worker forgets.
const STAND_INS: &str = "FAKE_";

/// Whether this build keeps [`STAND_INS`]'s.
const KEEPS_STAND_INS: bool = cfg!(any(test, feature = "testing"));

/// What the hub keeps of the environment it was started with. A variable whose name or value isn't text is left out,
/// as nothing the hub starts is given one.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Started {
    /// The OS session's, as [`SESSION`] and [`LOCALE`] name them.
    pub(super) session: Vars,
    /// The hub's own, as [`OWN`] names them, and in a test's build, a stand-in's, as [`STAND_INS`] does.
    pub(super) own: Vars,
    /// Its `PATH`, for when the profile's can't be read.
    pub(super) path: Option<String>,
}

impl Started {
    pub(super) fn keep(started_with: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        let mut started = Self::default();
        for (name, value) in started_with {
            let (Ok(name), Ok(value)) = (name.into_string(), value.into_string()) else { continue };
            if same_name(&name, "PATH") {
                started.path = Some(value);
            } else if SESSION.iter().any(|session| same_name(&name, session)) || starts(&name, LOCALE) {
                started.session.push((name, value));
            } else if starts(&name, OWN) || (KEEPS_STAND_INS && starts(&name, STAND_INS)) {
                started.own.push((name, value));
            }
        }
        started
    }

    /// The shell the user's session says is theirs.
    pub(super) fn shell(&self) -> Option<&str> {
        let shell = self.session.iter().find(|(name, _)| same_name(name, "SHELL"));
        shell.map(|(_, shell)| shell.as_str()).filter(|shell| !shell.is_empty())
    }
}

/// Whether `a` and `b` name the same variable, which on Windows doesn't depend on their case.
pub(super) fn same_name(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// Whether `name` starts with `start`, as [`same_name`] matches names.
fn starts(name: &str, start: &str) -> bool {
    name.get(..start.len()).is_some_and(|its| same_name(its, start))
}
