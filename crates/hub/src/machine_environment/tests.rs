use std::time::Duration;

use super::MachineEnvironment;
use super::environment::merged;
use super::login_shell::{PATH_MARKER, marked_path, read_until};

fn vars(vars: &[(&str, &str)]) -> Vec<(String, String)> {
    vars.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect()
}

/// The login shell's `PATH` comes first, then whatever of the hub's it lacks, as when a profile set it outright. With
/// nothing read, it's the hub's alone, and with no `PATH` at all, there's none.
#[test]
fn the_profiles_path_comes_first_and_keeps_what_the_hub_had() {
    let read = vars(&[("LANG", "en_GB.UTF-8"), ("PATH", "/profile/bin:/usr/bin")]);

    let merged_with_read = merged(read, Some("/hub/bin:/usr/bin"));

    let expected = [("LANG", "en_GB.UTF-8"), ("PATH", "/profile/bin:/usr/bin:/hub/bin")];
    assert_eq!(merged_with_read, vars(&expected));
    assert_eq!(merged(Vec::new(), Some("/hub/bin")), vars(&[("PATH", "/hub/bin")]));
    assert_eq!(merged(vars(&[("LANG", "C")]), None), vars(&[("LANG", "C")]));
}

/// As a provider's command for a worker gives variables of its own, which go over the machine's, and it takes nothing
/// out of what the command inherits.
#[test]
fn a_commands_own_variables_go_over_the_machines() {
    let machine = MachineEnvironment::only(&[("LANG", "C"), ("PATH", "/bin")]);
    let mut command = std::process::Command::new("worker");

    machine.with(&vars(&[("LANG", "en_GB.UTF-8")])).apply(&mut command);

    let os = std::ffi::OsStr::new;
    let mut given: Vec<_> = command.get_envs().collect();
    given.sort();
    assert_eq!(given, [(os("LANG"), Some(os("en_GB.UTF-8"))), (os("PATH"), Some(os("/bin")))]);
}

#[test]
fn the_path_survives_whatever_rc_files_print_around_it() {
    let printed = format!("Welcome!\n{PATH_MARKER}/a:/b\n{PATH_MARKER}\nbye\n");

    assert_eq!(marked_path(printed.as_bytes()).as_deref(), Some("/a:/b"));
    assert_eq!(marked_path(b"no markers"), None);
    assert_eq!(marked_path(format!("{PATH_MARKER}{PATH_MARKER}").as_bytes()), None);
}

#[tokio::test]
async fn reading_does_not_wait_for_a_stream_something_else_is_holding_open() {
    use tokio::io::AsyncWriteExt;

    let (mut writer, reader) = tokio::io::duplex(1024);
    writer.write_all(format!("noise {PATH_MARKER}/a:/b{PATH_MARKER}").as_bytes()).await.unwrap();

    // `writer` stays open, as it would with a background process started by an rc file.
    let said = |seen: &[u8]| marked_path(seen).is_some();
    let printed = tokio::time::timeout(Duration::from_secs(1), read_until(reader, said)).await.unwrap();

    assert_eq!(marked_path(&printed).as_deref(), Some("/a:/b"));
}

#[cfg(unix)]
mod login_shell {
    use std::path::{Path, PathBuf};

    use process::Environment;
    use process::testing::{Example, WholeEnvironment};

    use super::super::login_shell::deadline;
    use super::super::{LoginShell, MachineEnvironment};

    /// Stands in for the daemon printing what it was started with.
    static PRINTER: Example = Example::new("environment_printer");

    /// What `/bin/sh` reads as a login shell in `home`: a `.profile` that prints, as rc files can, adds to `PATH`,
    /// exports a variable built on one the hub had, and says where it started.
    const PROFILE: &str = "echo noise\nexport PATH=\"$HOME/profile-bin:$PATH\"\nexport BUILT_ON=\"$SET_FOR_THE_HUB, \
                           then the profile\"\nexport STARTED_IN=\"$(pwd)\"\n";

    /// A home with [`PROFILE`] in it.
    fn home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".profile"), PROFILE).unwrap();
        home
    }

    /// The stand-in, copied into a folder whose name has a space, as the daemon's copy in `Application Support` has.
    fn printer(dir: &Path) -> PathBuf {
        let folder = dir.join("with space");
        std::fs::create_dir_all(&folder).unwrap();
        let printer = folder.join("roundhoused");
        std::fs::copy(PRINTER.path(), &printer).unwrap();
        printer
    }

    /// What the hub would have in `home`, which has the test's `PATH` and a variable set for it, and nothing else of
    /// the test's own.
    fn hubs(home: &Path) -> Environment {
        WholeEnvironment::for_a_test(home).set("SET_FOR_THE_HUB", "the hub's").given()
    }

    /// `/bin/sh` reads `.profile` in its home as a login shell, started there, in the hub's environment, and runs the
    /// daemon, which prints what it has, but where the daemon is. The profile's `PATH` entry comes before the hub's.
    #[tokio::test]
    async fn the_login_shell_builds_on_the_hubs_environment() {
        let home = home();
        let given = hubs(home.path());
        let login = LoginShell { shell: "/bin/sh", home: Some(home.path()), given: &given };
        // The test's own, which `hubs` keeps.
        let hubs_path = std::env::var("PATH").unwrap();

        let machine = MachineEnvironment::read(Some(&login), &printer(home.path()), Some(&hubs_path)).await;

        assert_eq!(machine.get("BUILT_ON"), Some("the hub's, then the profile"));
        let started_in = machine.get("STARTED_IN").map(|dir| std::fs::canonicalize(dir).unwrap());
        assert_eq!(started_in, Some(std::fs::canonicalize(home.path()).unwrap()));
        let entries: Vec<PathBuf> = std::env::split_paths(machine.get("PATH").unwrap()).collect();
        let at = |entry: &Path| entries.iter().position(|its| its == entry);
        let hubs_first = std::env::split_paths(&hubs_path).next().unwrap();
        let (profiles, hubs) = (at(&home.path().join("profile-bin")), at(&hubs_first));
        assert!(profiles.zip(hubs).is_some_and(|(profiles, hubs)| profiles < hubs), "{entries:?}");
        for name in [process::ENVIRONMENT_PRINTER, "PWD", "SHLVL", "_"] {
            assert_eq!(machine.get(name), None, "it has {name}");
        }
    }

    /// A shell that can't run the daemon still says its `PATH`, which goes before the hub's. One that can't start says
    /// nothing, so only the hub's is given.
    #[tokio::test]
    async fn a_shell_that_cant_run_the_daemon_still_gives_its_path() {
        let home = home();
        let given = hubs(home.path());
        let login = LoginShell { shell: "/bin/sh", home: Some(home.path()), given: &given };
        let unstarted = LoginShell { shell: "/no/such/shell", ..login };
        let nowhere = home.path().join("no-printer-here");

        let ran = MachineEnvironment::read(Some(&login), &nowhere, Some("/hub/bin")).await;
        let didnt = MachineEnvironment::read(Some(&unstarted), &nowhere, Some("/hub/bin")).await;

        let path = ran.get("PATH").unwrap();
        let profiles = home.path().join("profile-bin");
        assert!(path.starts_with(profiles.to_str().unwrap()) && path.ends_with(":/hub/bin"), "{path}");
        assert_eq!(ran.get("BUILT_ON"), None);
        assert_eq!(didnt.get("PATH"), Some("/hub/bin"));
    }

    /// Runs the person's own login shell, in this test's own environment, so this also covers their rc files, which
    /// can print, and how their shell runs a program whose path has a space, which fish quotes its own way.
    #[tokio::test]
    async fn the_accounts_login_shell_really_says_its_environment() {
        let dir = tempfile::tempdir().unwrap();
        let shell = process::account_shell().unwrap_or_else(|| "/bin/sh".to_string());
        let given = Environment::NONE;
        let login = LoginShell { shell: &shell, home: None, given: &given };

        let read = login.environment(&printer(dir.path()), deadline()).await;

        // Only the names, since the values can hold the person's secrets.
        let names: Vec<&str> = read.iter().flatten().map(|(name, _)| name.as_str()).collect();
        let path = read.iter().flatten().find(|(name, _)| name == "PATH");
        assert!(path.is_some_and(|(_, path)| path.contains('/')), "{shell} said {names:?}");
    }
}
