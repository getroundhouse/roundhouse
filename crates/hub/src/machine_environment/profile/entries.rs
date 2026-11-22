//! Reading the variables a login shell or Windows says the profile gives, each `NAME=value`, ended by a NUL.

use super::super::Vars;
use super::super::kept::same_name;
#[cfg(any(unix, test))]
use super::super::printing::MARKER;

/// What the shell sets for itself, about where it is and how deep, rather than what the profile gives.
const THE_SHELLS_OWN: [&str; 4] = ["PWD", "OLDPWD", "SHLVL", "_"];

/// What's between the daemon's two [`MARKER`]s in what the login shell printed, or `None` before there are two. The
/// second starts a variable, rather than being anywhere in one, so a variable whose value holds the marker doesn't end
/// what's read before it.
#[cfg(any(unix, test))]
pub(in crate::machine_environment) fn marked(printed: &[u8]) -> Option<&[u8]> {
    let marker = MARKER.as_bytes();
    let marks = |at: usize| printed.get(at..).is_some_and(|rest| rest.starts_with(marker));
    let start = (0..printed.len()).find(|&at| marks(at))? + marker.len();
    let starts_a_variable = |at: usize| at == start || printed.get(at - 1) == Some(&0);
    let end = (start..=printed.len()).find(|&at| starts_a_variable(at) && marks(at))?;
    printed.get(start..end)
}

/// The variables in what the login shell printed between its markers, each ended by a NUL, as the daemon prints them.
/// One that isn't text is left out, as is one the shell sets for itself.
#[cfg(any(unix, test))]
pub(in crate::machine_environment) fn from_shell(printed: &[u8]) -> Option<Vars> {
    let entries = marked(printed)?.split(|byte| *byte == 0).filter_map(|entry| std::str::from_utf8(entry).ok());
    Some(entries.filter_map(entry).collect())
}

/// The variables in a Windows environment block, as UTF-16 text, each ended by a NUL. One that isn't text is left out,
/// as are those that name a drive's current directory, such as `=C:`, which have no name before their `=`.
#[cfg(any(windows, test))]
pub(in crate::machine_environment) fn from_block(block: &[u16]) -> Vars {
    let entries = block.split(|unit| *unit == 0).filter_map(|entry| String::from_utf16(entry).ok());
    entries.filter_map(|entry| self::entry(&entry)).collect()
}

/// `NAME=value`, split at its first `=`, unless it has no name or the shell set it for itself.
fn entry(entry: &str) -> Option<(String, String)> {
    let (name, value) = entry.split_once('=')?;
    let wanted = !name.is_empty() && !THE_SHELLS_OWN.iter().any(|own| same_name(name, own));
    wanted.then(|| (name.to_string(), value.to_string()))
}
