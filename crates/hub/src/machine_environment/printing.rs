//! How the daemon prints the environment the login shell started it in, for the hub to read, so reading it rests on
//! nothing of the machine's own, such as an `env` that takes `-0`, which macOS's and `BusyBox`'s may not.

use std::ffi::OsString;
use std::io::{self, Write};

/// The daemon's role that prints its environment, as [`print_environment`] says. A hub runs it from its login shell.
pub const PRINTS_ENVIRONMENT: &str = "print-environment";

/// Where the daemon that prints the environment is, which the hub gives its login shell, so its path is never written
/// into the shell's command, where each shell would want it quoted its own way.
pub(super) const PRINTER: &str = "ROUNDHOUSE_ENVIRONMENT_PRINTER";

/// What marks the printed environment at each end, since rc files can print anything before it. Only the daemon prints
/// it, never the shell, since a shell can hold the last word of a command it ran in a variable, as dash's `_` does, and
/// a profile can export that, as one that sources a file under `set -a` does.
pub(super) const MARKER: &str = "__ROUNDHOUSE_PROFILE_ENVIRONMENT__";

/// Writes each of `vars` to `out` as `NAME=value`, ended by a NUL, since a value can hold a newline, between two
/// [`MARKER`]s, but for [`PRINTER`], which the hub set only to start this.
pub fn print_environment(vars: impl IntoIterator<Item = (OsString, OsString)>, out: &mut impl Write) -> io::Result<()> {
    out.write_all(MARKER.as_bytes())?;
    for (name, value) in vars.into_iter().filter(|(name, _)| name != PRINTER) {
        out.write_all(name.as_encoded_bytes())?;
        out.write_all(b"=")?;
        out.write_all(value.as_encoded_bytes())?;
        out.write_all(b"\0")?;
    }
    out.write_all(MARKER.as_bytes())?;
    out.flush()
}
