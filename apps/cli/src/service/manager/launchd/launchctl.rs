//! Writing the launch daemon, and loading and unloading it with `launchctl`.

use std::path::Path;
use std::process::Command;

use anyhow::Context;

use super::property_list::launch_daemon;
use crate::service::definition::Service;
use crate::service::manager::command::run;
use crate::service::manager::system_file;

/// The launch daemon's label, under the app's bundle identifier's domain.
const LABEL: &str = "dev.getroundhouse.hub";
/// The system's domain, where a launch daemon runs, and the service's name in it.
const TARGET: &str = "system/dev.getroundhouse.hub";
const PLIST: &str = "/Library/LaunchDaemons/dev.getroundhouse.hub.plist";

/// Writes the launch daemon and starts it, stopping the one it replaces first. The workers of the hub it stops keep
/// running, since each runs in a session of its own, which launchd leaves alone, as `docs/reference/launchd.md` says.
pub(crate) fn register(service: &Service) -> anyhow::Result<()> {
    system_file::write(Path::new(PLIST), &launch_daemon(service, LABEL))?;
    if is_loaded()? {
        run("launchctl", &["bootout", TARGET])?;
    }
    // In case someone disabled it, which would keep it from starting at boot.
    run("launchctl", &["enable", TARGET])?;
    run("launchctl", &["bootstrap", "system", PLIST])
}

/// Stops the launch daemon and removes it.
pub(crate) fn unregister() -> anyhow::Result<()> {
    anyhow::ensure!(Path::new(PLIST).exists(), "no hub is installed as a service here");
    if is_loaded()? {
        run("launchctl", &["bootout", TARGET])?;
    }
    std::fs::remove_file(PLIST).with_context(|| format!("removing {PLIST}"))
}

/// Whether launchd has the launch daemon loaded, which `launchctl print` answers only for one it has.
fn is_loaded() -> anyhow::Result<bool> {
    let output = Command::new("launchctl").args(["print", TARGET]).output().context("running launchctl")?;
    Ok(output.status.success())
}
