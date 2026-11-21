//! The systemd unit.

use std::fmt::Write as _;

use crate::service::definition::{ARGUMENTS, Service};

/// The systemd unit. It starts the hub at boot, and again if it exits with a failure, but not when it stops because
/// someone asked. `KillMode=process` stops only the hub with the service, since systemd otherwise stops every process
/// the service started, its workers and their agents too.
pub(super) fn systemd_unit(service: &Service) -> String {
    let mut environment = String::new();
    for (name, value) in &service.environment {
        // Writing to a `String` can't fail.
        let _ = writeln!(environment, "Environment={}", quoted(&format!("{name}={value}")));
    }
    format!(
        "[Unit]
Description=Roundhouse hub
Wants=network-online.target
After=network-online.target

[Service]
User={user}
ExecStart={program} {arguments}
WorkingDirectory={dir}
{environment}StandardOutput=append:{log}
StandardError=append:{log}
Restart=on-failure
KillMode=process

[Install]
WantedBy=multi-user.target
",
        user = unquoted(&service.user),
        program = quoted(&service.program.to_string_lossy()),
        arguments = ARGUMENTS.map(quoted).join(" "),
        dir = unquoted(&service.working_dir.to_string_lossy()),
        log = unquoted(&service.log.to_string_lossy()),
    )
}

/// A value systemd takes whole, with its specifiers' `%` doubled, and in quotes, for a setting that splits at spaces.
fn quoted(text: &str) -> String {
    format!("\"{}\"", unquoted(text).replace('\\', "\\\\").replace('"', "\\\""))
}

/// A value systemd takes as it is to the end of its line, but for its specifiers, which start with `%`.
fn unquoted(text: &str) -> String {
    text.replace('%', "%%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::definition::escaping;

    #[test]
    fn a_systemd_unit_runs_the_hub_as_its_user_and_stops_only_the_hub() {
        let unit = systemd_unit(&escaping());

        assert!(unit.contains("\nUser=agent\n"), "{unit}");
        let program = "\"/Users/a & b/Library/Application Support/roundhouse/service/current/roundhoused\"";
        assert!(unit.contains(&format!("\nExecStart={program} \"hub\" \"--service\"\n")), "{unit}");
        assert!(unit.contains("\nEnvironment=\"PATH=/usr/bin:/opt/100%%\\\"x\\\"\"\n"), "{unit}");
        assert!(unit.contains("\nRestart=on-failure\n"), "{unit}");
        assert!(unit.contains("\nKillMode=process\n"), "{unit}");
    }
}
