//! How a program prints the environment it was started in, for the program that started it to read, which a hub has
//! its login shell do: the daemon prints, and the hub reads. Reading it rests on nothing of the machine's own, such as
//! an `env` that takes `-0`, which macOS's and `BusyBox`'s may not.

use std::ffi::OsString;
use std::io::{self, Write};

/// What a program is asked, as its first argument, to print its environment, as [`print_environment`] says. The
/// daemon has a role of that name.
pub const PRINTS_ENVIRONMENT: &str = "print-environment";

/// Where the program that prints the environment is, which a hub gives its login shell, so its path is never written
/// into the shell's command, where each shell would want it quoted its own way. Every shell expands a variable in
/// double quotes the same way, fish's too.
pub const ENVIRONMENT_PRINTER: &str = "ROUNDHOUSE_ENVIRONMENT_PRINTER";

/// What marks the printed environment at each end, since rc files can print anything before it. Only the program
/// prints it, never the shell, since a shell can hold the last word of a command it ran in a variable, as dash's `_`
/// does, and a profile can export that, as one that sources a file under `set -a` does.
const MARKER: &str = "__ROUNDHOUSE_PROFILE_ENVIRONMENT__";

/// Writes each of `vars` to `out` as `NAME=value`, ended by a NUL, since a value can hold a newline, between two
/// markers, but for [`ENVIRONMENT_PRINTER`], which was set only to start this.
pub fn print_environment(vars: impl IntoIterator<Item = (OsString, OsString)>, out: &mut impl Write) -> io::Result<()> {
    out.write_all(MARKER.as_bytes())?;
    for (name, value) in vars.into_iter().filter(|(name, _)| name != ENVIRONMENT_PRINTER) {
        out.write_all(name.as_encoded_bytes())?;
        out.write_all(b"=")?;
        out.write_all(value.as_encoded_bytes())?;
        out.write_all(b"\0")?;
    }
    out.write_all(MARKER.as_bytes())?;
    out.flush()
}

/// The variables [`print_environment`] printed, among whatever else `printed` holds, such as what rc files printed
/// around it, in the order printed. `None` until both markers are there. One that isn't text is left out.
pub fn printed_environment(printed: &[u8]) -> Option<Vec<(String, String)>> {
    let entries = marked(printed)?.split(|byte| *byte == 0).filter_map(|entry| std::str::from_utf8(entry).ok());
    let variable = |entry: &str| {
        let (name, value) = entry.split_once('=')?;
        (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
    };
    Some(entries.filter_map(variable).collect())
}

/// What's between the two markers in `printed`, or `None` before there are two. The second starts a variable, rather
/// than being anywhere in one, so a variable whose value holds the marker doesn't end what's read before it.
fn marked(printed: &[u8]) -> Option<&[u8]> {
    let marker = MARKER.as_bytes();
    let marks = |at: usize| printed.get(at..).is_some_and(|rest| rest.starts_with(marker));
    let start = (0..printed.len()).find(|&at| marks(at))? + marker.len();
    let starts_a_variable = |at: usize| at == start || printed.get(at - 1) == Some(&0);
    let end = (start..=printed.len()).find(|&at| starts_a_variable(at) && marks(at))?;
    printed.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(vars: &[(&str, &str)]) -> Vec<(String, String)> {
        vars.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect()
    }

    /// What's printed reads back as it was, among what rc files printed around it, with a value that holds a newline,
    /// but for where the printer is, which was given only to start it. That includes dash's `_`, the last word of the
    /// command it ran before, which a profile that sources a file under `set -a` exports, so it was the marker when the
    /// shell printed the marker itself.
    #[test]
    fn what_is_printed_reads_back_but_for_where_the_printer_is() {
        let given = [("_", MARKER), ("NOTE", "two\nlines"), (ENVIRONMENT_PRINTER, "/app/roundhoused")];
        let mut printed = b"what an rc file printed\n".to_vec();

        print_environment(given.map(|(name, value)| (name.into(), value.into())), &mut printed).unwrap();
        printed.extend_from_slice(b"bye\n");

        assert_eq!(printed_environment(&printed), Some(vars(&[("_", MARKER), ("NOTE", "two\nlines")])));
    }

    #[test]
    fn nothing_is_read_before_both_markers() {
        assert_eq!(printed_environment(b"no markers"), None);
        assert_eq!(printed_environment(format!("{MARKER}PATH=/a\0").as_bytes()), None);
        assert_eq!(printed_environment(format!("{MARKER}{MARKER}").as_bytes()), Some(Vec::new()));
    }
}
