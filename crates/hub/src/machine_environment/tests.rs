use std::ffi::OsString;

use super::environment::whole;
use super::kept::Started;
use super::printing::MARKER;
use super::profile::{from_block, from_shell};
use super::{MachineEnvironment, Vars, print_environment};

fn vars(vars: &[(&str, &str)]) -> Vars {
    vars.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect()
}

/// A Claude session's, tmux's and a virtualenv's go, as does one of the person's own that only the terminal the hub was
/// started from had. The session's, the locale's and the hub's own stay, and in a test's build, a stand-in's.
#[test]
fn the_hub_keeps_only_the_sessions_and_its_own_of_what_it_was_started_with() {
    let path = if cfg!(windows) { r"C:\bin" } else { "/bin" };
    let given = [
        ("CLAUDECODE", "1"),
        ("TMUX", "/tmp/tmux-501/default,1,0"),
        ("VIRTUAL_ENV", "/work/.venv"),
        ("GH_TOKEN", "the-terminals-own"),
        ("TMP", "/tmp"),
        ("LC_CTYPE", "UTF-8"),
        ("ROUNDHOUSE_CLAUDE_BIN", "/opt/claude"),
        ("FAKE_CLAUDE_STATE", "/tmp/agent-state"),
        ("PATH", path),
    ];

    let started = Started::keep(given.iter().map(|(name, value)| (OsString::from(name), OsString::from(value))));

    let session =
        if cfg!(windows) { vars(&[("TMP", "/tmp"), ("LC_CTYPE", "UTF-8")]) } else { vars(&[("LC_CTYPE", "UTF-8")]) };
    let own = vars(&[("ROUNDHOUSE_CLAUDE_BIN", "/opt/claude"), ("FAKE_CLAUDE_STATE", "/tmp/agent-state")]);
    assert_eq!(started, Started { session, own, path: Some(path.to_string()) });
}

#[cfg(unix)]
#[test]
fn the_unix_sessions_own_are_kept_by_name() {
    let names = ["HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "SSH_AUTH_SOCK", "XDG_RUNTIME_DIR", "DISPLAY"];
    let given = names.iter().map(|name| (OsString::from(name), OsString::from("kept")));

    let started = Started::keep(given.chain([(OsString::from("TERM_PROGRAM"), OsString::from("Apple_Terminal"))]));

    let kept: Vec<&str> = started.session.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(kept, names);
    assert_eq!(started.shell(), Some("kept"));
}

/// What the daemon printed, between markers, among what rc files printed around it, with a value that holds a newline.
#[test]
fn the_profiles_variables_survive_whatever_rc_files_print_around_them() {
    let printed =
        format!("Welcome!\n{MARKER}PATH=/a:/b\0SHLVL=1\0PWD=/hub\0NOTE=two\nlines\0_=/usr/bin/env\0{MARKER}bye\n");

    let read = from_shell(printed.as_bytes());

    assert_eq!(read, Some(vars(&[("PATH", "/a:/b"), ("NOTE", "two\nlines")])));
    assert_eq!(from_shell(b"no markers"), None);
    assert_eq!(from_shell(format!("{MARKER}{MARKER}").as_bytes()), Some(Vars::new()));
}

/// What the daemon prints reads back as it was, after what rc files printed, with a value that holds a newline, but for
/// where the daemon is, which the hub gave the shell only to start it, and what the shell sets for itself. That includes
/// dash's `_`, the last word of the command it ran before, which a profile that sources a file under `set -a` exports,
/// so it was the marker when the shell printed that itself.
#[test]
fn what_the_daemon_prints_reads_back_but_for_where_it_is() {
    let given = [("_", MARKER), ("NOTE", "two\nlines"), ("ROUNDHOUSE_ENVIRONMENT_PRINTER", "/app/roundhoused")];
    let mut printed = b"what an rc file printed\n".to_vec();

    print_environment(given.map(|(name, value)| (name.into(), value.into())), &mut printed).unwrap();

    assert_eq!(from_shell(&printed), Some(vars(&[("NOTE", "two\nlines")])));
}

/// Each drive's current directory is in a block as `=C:=C:\…`, which isn't a variable.
#[test]
fn a_windows_block_is_read_but_for_its_drives_directories() {
    let block: Vec<u16> =
        "=C:=C:\\work\0Path=C:\\Windows;C:\\bin\0USERPROFILE=C:\\Users\\ada\0\0".encode_utf16().collect();

    let read = from_block(&block);

    assert_eq!(read, vars(&[("Path", r"C:\Windows;C:\bin"), ("USERPROFILE", r"C:\Users\ada")]));
}

#[test]
fn the_profiles_go_over_the_sessions_and_the_hubs_own_over_both() {
    let started = Started {
        session: vars(&[("HOME", "/home/ada"), ("LANG", "C")]),
        own: vars(&[("ROUNDHOUSE_LOG", "debug")]),
        path: Some("/started/bin".to_string()),
    };
    let profile = vars(&[("LANG", "en_GB.UTF-8"), ("PATH", "/profile/bin"), ("ROUNDHOUSE_LOG", "info")]);

    let read = whole(&started, Some(profile));
    let unread = whole(&started, None);

    let both = [("HOME", "/home/ada"), ("LANG", "en_GB.UTF-8"), ("PATH", "/profile/bin"), ("ROUNDHOUSE_LOG", "debug")];
    assert_eq!(read, vars(&both));
    let session_and_path =
        [("HOME", "/home/ada"), ("LANG", "C"), ("PATH", "/started/bin"), ("ROUNDHOUSE_LOG", "debug")];
    assert_eq!(unread, vars(&session_and_path));
}

/// As a provider's command for a worker gives variables of its own, which go over the machine's. What the command
/// inherits is taken out, as Cargo's `CARGO_MANIFEST_DIR` for this test.
#[tokio::test]
async fn a_commands_own_variables_go_over_the_machines_and_nothing_else_is_inherited() {
    let machine = MachineEnvironment::only(&[("LANG", "C"), ("PATH", "/bin")]);
    let mut command = std::process::Command::new("worker");
    command.env("SET_BEFORE", "kept");

    machine.given(&vars(&[("LANG", "en_GB.UTF-8")])).await.apply(&mut command);

    let os = std::ffi::OsStr::new;
    let given: Vec<_> = command.get_envs().filter(|(_, value)| value.is_some()).collect();
    let expected = [("LANG", "en_GB.UTF-8"), ("PATH", "/bin"), ("SET_BEFORE", "kept")];
    assert_eq!(given, expected.map(|(name, value)| (os(name), Some(os(value)))));
    assert!(command.get_envs().any(|(name, value)| name == "CARGO_MANIFEST_DIR" && value.is_none()));
}

#[cfg(unix)]
mod login_shell {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::super::Vars;
    use super::super::printing::MARKER;
    use super::super::profile::{from_shell, read, read_until_second_marker};
    use super::vars;

    /// A stand-in for the daemon printing its environment, in a folder whose name has a space, as an app's in
    /// `Application Support` has, which prints only the variables these tests are about, with what the shell has built
    /// in, and `print-environment` once it's asked that, between the markers, as the daemon prints them.
    fn printer(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = dir.join("with space");
        std::fs::create_dir_all(&folder).unwrap();
        let printer = folder.join("roundhoused");
        let script = format!(
            r#"#!/bin/sh
[ "$1" = print-environment ] || exit 9
printf '{MARKER}ASKED=%s\0' "$1"
for name in HOME PATH FROM_PROFILE CARGO_MANIFEST_DIR; do
  eval "[ -n \"\${{$name+set}}\" ] && printf '%s=%s\0' $name \"\$$name\""
done
printf '{MARKER}'
"#
        );
        std::fs::write(&printer, script).unwrap();
        std::fs::set_permissions(&printer, std::fs::Permissions::from_mode(0o755)).unwrap();
        printer
    }

    /// `/bin/sh` reads `.profile` in its home as a login shell, and runs the daemon. Cargo sets `CARGO_MANIFEST_DIR`
    /// for this test, and it isn't in the session, so it doesn't come back through the shell.
    #[tokio::test]
    async fn the_login_shell_runs_with_only_the_session_and_says_what_the_profile_exports() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".profile"), "echo noise\nexport FROM_PROFILE='two\nlines'\n").unwrap();
        let session = vars(&[("HOME", home.path().to_str().unwrap())]);

        let read = read(Some("/bin/sh"), &session, &printer(home.path())).await.unwrap();

        let has = |name: &str| read.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str());
        assert_eq!(has("FROM_PROFILE"), Some("two\nlines"), "it read {read:?}");
        assert_eq!(has("HOME"), home.path().to_str());
        assert_eq!(has("CARGO_MANIFEST_DIR"), None);
        assert_eq!(has("ASKED"), Some("print-environment"));
    }

    /// Runs the person's own shell, so this also covers rc files that print to stdout, and how their shell runs a
    /// program whose path has a space, which fish quotes its own way.
    #[tokio::test]
    async fn the_accounts_login_shell_really_says_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let session: Vars = std::env::var("HOME").map(|home| ("HOME".to_string(), home)).into_iter().collect();

        let read = read(None, &session, &printer(dir.path())).await;

        let path = read.iter().flatten().find(|(name, _)| name == "PATH");
        // Only the names, since the values can hold the person's secrets.
        let names: Vec<&str> = read.iter().flatten().map(|(name, _)| name.as_str()).collect();
        assert!(path.is_some_and(|(_, path)| path.contains('/')), "got {names:?}");
    }

    #[tokio::test]
    async fn reading_does_not_wait_for_a_stream_something_else_is_holding_open() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, reader) = tokio::io::duplex(1024);
        writer.write_all(format!("noise {MARKER}PATH=/a:/b\0{MARKER}").as_bytes()).await.unwrap();
        // `writer` stays open, as it would with a background process started by an rc file.
        let printed = tokio::time::timeout(Duration::from_secs(1), read_until_second_marker(reader)).await.unwrap();

        assert_eq!(printed.as_deref().and_then(from_shell), Some(vars(&[("PATH", "/a:/b")])));
    }
}
