//! Whether this machine's git reads the git config the worker gives every process, from what `git --version` says.

/// The first git to read `GIT_CONFIG_COUNT`, which is how every process is given the worker's git config.
const FIRST: (u32, u32) = (2, 31);

/// `Ok` for a git that reads it, from what `git --version` printed, such as `git version 2.54.0 (Apple Git-157)` or
/// `git version 2.45.1.windows.1`, and otherwise why not, for the hub to hear.
pub(super) fn reads_the_config(said: &str) -> Result<(), String> {
    let version = said.trim().strip_prefix("git version ").unwrap_or_default();
    let mut parts = version.split('.').map(|part| part.split(|c: char| !c.is_ascii_digit()).next().unwrap_or_default());
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next().map(str::parse::<u32>), parts.next().map(str::parse::<u32>))
    else {
        return Err(format!("couldn't tell which version git is from {:?}", said.trim()));
    };
    if (major, minor) < FIRST {
        let (first_major, first_minor) = FIRST;
        return Err(format!(
            "git {version} is older than {first_major}.{first_minor}, the first to read GIT_CONFIG_COUNT"
        ));
    }
    Ok(())
}
