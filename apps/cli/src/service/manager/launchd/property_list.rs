//! The launch daemon's property list.

use std::fmt::Write as _;
use std::path::Path;

use crate::service::definition::Service;

/// The launch daemon's property list. It starts the hub at boot, and again if it exits with a failure, but not when it
/// stops because someone asked, so `roundhouse hub stop` keeps it stopped until the next boot.
pub(super) fn launch_daemon(service: &Service, label: &str) -> String {
    let mut environment = String::new();
    for (name, value) in &service.environment {
        // Writing to a `String` can't fail.
        let _ = write!(environment, "\n    <key>{}</key>{}", escaped(name), string(value));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>{label}
  <key>UserName</key>{user}
  <key>ProgramArguments</key>
  <array>{program}{hub}</array>
  <key>WorkingDirectory</key>{dir}
  <key>EnvironmentVariables</key>
  <dict>{environment}
  </dict>
  <key>StandardOutPath</key>{log}
  <key>StandardErrorPath</key>{log}
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key><false/>
  </dict>
</dict>
</plist>
"#,
        label = string(label),
        user = string(&service.user),
        program = path(&service.program),
        hub = string("hub"),
        dir = path(&service.working_dir),
        log = path(&service.log),
    )
}

fn string(text: &str) -> String {
    format!("<string>{}</string>", escaped(text))
}

fn path(path: &Path) -> String {
    string(&path.to_string_lossy())
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::definition::escaping;

    #[test]
    fn a_launch_daemon_runs_the_hub_as_its_user_and_restarts_it_only_after_a_failure() {
        let plist = launch_daemon(&escaping(), "dev.getroundhouse.hub");

        assert!(plist.contains("<key>Label</key><string>dev.getroundhouse.hub</string>"), "{plist}");
        assert!(plist.contains("<key>UserName</key><string>agent</string>"), "{plist}");
        let program =
            "<string>/Users/a &amp; b/Library/Application Support/roundhouse/service/current/roundhoused</string>";
        assert!(plist.contains(&format!("<array>{program}<string>hub</string></array>")), "{plist}");
        assert!(plist.contains("<key>HOME</key><string>/Users/a &amp; b</string>"), "{plist}");
        assert!(plist.contains("<key>SuccessfulExit</key><false/>"), "{plist}");
        assert!(!plist.contains("a & b"), "{plist}");
    }

    /// `plutil`, which every Mac has, reads it as a property list.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_launch_daemon_is_a_property_list_macos_reads() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hub.plist");
        std::fs::write(&file, launch_daemon(&escaping(), "dev.getroundhouse.hub")).unwrap();

        let linted = std::process::Command::new("/usr/bin/plutil").arg("-lint").arg(&file).output().unwrap();

        assert!(linted.status.success(), "{}", String::from_utf8_lossy(&linted.stdout));
    }
}
