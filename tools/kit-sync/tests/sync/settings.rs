//! The person's git settings, which change nothing the tool does in its own repositories.

use std::ffi::OsString;
use std::fs;

use kit_sync::{Git, Synced};

use crate::fixture::{Stand, lines, write};

/// A hook that fails, so a commit that runs it fails.
fn failing_hook(dir: &std::path::Path) {
    write(dir, &[("pre-commit", "#!/bin/sh\nexit 1\n")]);
    set_runnable(&dir.join("pre-commit"));
}

#[cfg(unix)]
fn set_runnable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Windows runs a hook by its name, whatever its permissions.
#[cfg(windows)]
fn set_runnable(_: &std::path::Path) {}

#[test]
fn the_persons_git_settings_change_nothing_it_does() {
    let stand = Stand::new();
    failing_hook(&stand.work.join("hooks"));
    failing_hook(&stand.work.join("template/hooks"));
    let hooks = stand.work.join("hooks").display().to_string();
    write(
        &stand.work,
        &[
            (
                "gitconfig",
                &format!(
                    "[merge]\n\tff = only\n[diff]\n\tnoprefix = true\n[color]\n\tui = always\n\tdiff = always\n\
                 [core]\n\thooksPath = {hooks}\n[commit]\n\tgpgSign = true\n"
                ),
            ),
            // Read from ~/.config/git whatever the settings say.
            ("xdg/git/attributes", "*.rs binary\n"),
        ],
    );
    let in_work = |name: &str| stand.work.join(name).into_os_string();
    let environment: Vec<(OsString, OsString)> = vec![
        ("GIT_CONFIG_GLOBAL".into(), in_work("gitconfig")),
        ("XDG_CONFIG_HOME".into(), in_work("xdg")),
        ("GIT_CONFIG_PARAMETERS".into(), "'merge.ff'='only'".into()),
        ("GIT_TEMPLATE_DIR".into(), in_work("template")),
        ("GIT_DIR".into(), in_work("nowhere")),
    ];
    stand.sync_in(&environment, "v1", &stand.first).unwrap();
    stand.commit_root();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "ours")]))]);
    write(&stand.upstream, &[("crates/base/src/lib.rs", &lines(12, &[(10, "upstream's")]))]);
    let second = stand.release("v2");

    let synced = stand.sync_in(&environment, "v2", &second).unwrap();

    assert_eq!(synced, Synced::Brought { moved: None });
    assert_eq!(stand.read("crates/base/src/lib.rs"), lines(12, &[(2, "ours"), (10, "upstream's")]));
    let git = Git::new(std::env::vars_os().chain(environment));
    let upstream = stand.pin("v2", &second, Vec::new()).upstream;
    let patch = kit_sync::diff(&git, &stand.root, &upstream, &[]).unwrap();
    assert!(patch.contains("--- a/crates/base/src/lib.rs\n"), "{patch}");
}
