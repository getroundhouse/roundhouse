//! Writing the unit, and enabling, starting and removing it with `systemctl`.

use std::path::Path;

use anyhow::Context;

use super::unit_file::systemd_unit;
use crate::service::definition::Service;
use crate::service::manager::command::run;
use crate::service::manager::system_file;

const UNIT: &str = "roundhouse-hub.service";
const UNIT_FILE: &str = "/etc/systemd/system/roundhouse-hub.service";

/// Writes the unit, and starts it, restarting the one it replaces. The workers of the hub that stops keep running,
/// since the unit has systemd stop only the hub.
pub(crate) fn register(service: &Service) -> anyhow::Result<()> {
    system_file::write(Path::new(UNIT_FILE), &systemd_unit(service))?;
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", UNIT])?;
    run("systemctl", &["restart", UNIT])
}

/// Stops the service and removes it.
pub(crate) fn unregister() -> anyhow::Result<()> {
    anyhow::ensure!(Path::new(UNIT_FILE).exists(), "no hub is installed as a service here");
    run("systemctl", &["disable", "--now", UNIT])?;
    std::fs::remove_file(UNIT_FILE).with_context(|| format!("removing {UNIT_FILE}"))?;
    run("systemctl", &["daemon-reload"])
}
