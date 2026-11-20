//! Windows, which runs no hub as a service yet.

use crate::service::definition::Service;

/// Refused, since a Windows service isn't built.
pub(crate) fn register(_service: &Service) -> anyhow::Result<()> {
    anyhow::bail!("installing a hub as a service isn't built on Windows yet")
}

/// Refused, since a Windows service isn't built.
pub(crate) fn unregister() -> anyhow::Result<()> {
    anyhow::bail!("installing a hub as a service isn't built on Windows yet")
}
