use std::fs::Permissions;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};

use super::*;
use crate::stand_in_hub::{self, Does};

const VERSION: &str = "abc123";

/// Stands in for a daemon of build `version`, and runs `body` when started as anything else.
fn daemon_script(version: &str, body: &str) -> String {
    format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo \"{DAEMON_NAME} {version}\"; exit; fi\n{body}\n")
}

/// Writes down which file it was started from.
const SAYS_WHERE_IT_RAN: &str = "echo \"$0\" > \"$ROUNDHOUSE_HOME/started-from\"";

pub(super) fn paths_in(dir: &Path) -> Paths {
    protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(dir.join("home").to_string_lossy().into_owned()),
        protocol::paths::ENV_WORKSPACES => Some(dir.join("workspaces").to_string_lossy().into_owned()),
        _ => None,
    })
    .unwrap()
}

/// Where an app's install would keep it. Through a rename, as an update replaces it.
fn install(dir: &Path, name: &str, script: &str) -> PathBuf {
    let app = dir.join(name);
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("new"), script).unwrap();
    std::fs::set_permissions(app.join("new"), Permissions::from_mode(0o755)).unwrap();
    std::fs::rename(app.join("new"), app.join(DAEMON_NAME)).unwrap();
    app.join(DAEMON_NAME)
}

pub(super) fn build(number: u64) -> StampedBuild {
    StampedBuild::new(VERSION, number).unwrap()
}

fn stamped(original: &Path, number: u64) -> Daemon {
    Daemon { path: original.to_path_buf(), build: Some(build(number)) }
}

/// Where build 1's copy goes, with any temporary files beside it.
fn build_dir(paths: &Paths) -> PathBuf {
    paths.hub_copy(&build(1)).parent().unwrap().to_path_buf()
}

pub(super) fn files_in(dir: &Path) -> Vec<String> {
    let mut files: Vec<_> =
        std::fs::read_dir(dir).unwrap().map(|file| file.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    files
}

fn wait_for<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What [`SAYS_WHERE_IT_RAN`] wrote, once it has.
fn started_from(paths: &Paths) -> String {
    let file = paths.data_dir.join("started-from");
    wait_for("the daemon to run", || std::fs::read_to_string(&file).ok()?.strip_suffix('\n').map(str::to_string))
}

/// A copy reading its original from a pipe, so it can be caught halfway.
struct CopyFromPipe {
    pipe: File,
    copying: std::thread::JoinHandle<io::Result<PathBuf>>,
}

impl CopyFromPipe {
    /// Returns once `first` has been copied.
    fn start(dir: &Path, paths: &Paths, first: &[u8]) -> Self {
        let original = dir.join("pipe");
        assert!(Command::new("mkfifo").arg(&original).status().unwrap().success());
        let copying = std::thread::spawn({
            let (daemon, paths) = (stamped(&original, 1), paths.clone());
            move || daemon.to_start(&paths)
        });
        // Without blocking, which fails until the copy is reading from the other end.
        let mut pipe = wait_for("the copy to open the original", || {
            OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&original).ok()
        });
        pipe.write_all(first).unwrap();
        let length = u64::try_from(first.len()).unwrap();
        wait_for("the first part to be copied", || {
            let mut files = std::fs::read_dir(build_dir(paths)).ok()?;
            files.any(|file| file.is_ok_and(|f| f.metadata().is_ok_and(|m| m.len() == length))).then_some(())
        });
        Self { pipe, copying }
    }

    fn finish(self, rest: &[u8]) -> io::Result<PathBuf> {
        let Self { mut pipe, copying } = self;
        pipe.write_all(rest).unwrap();
        drop(pipe);
        copying.join().unwrap()
    }
}

#[test]
fn a_stamped_build_starts_its_hub_from_a_copy_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let script = daemon_script(VERSION, SAYS_WHERE_IT_RAN);
    let original = install(dir.path(), "app", &script);

    start_hub(&paths, &stamped(&original, 1)).unwrap();

    let copy = paths.hub_copy(&build(1));
    assert_eq!(started_from(&paths), copy.to_str().unwrap());
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), script);
}

#[test]
fn any_other_build_starts_the_daemon_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = install(dir.path(), "app", &daemon_script(VERSION, SAYS_WHERE_IT_RAN));

    start_hub(&paths, &Daemon::in_place(&original)).unwrap();

    assert_eq!(started_from(&paths), original.to_str().unwrap());
    assert!(!paths.daemons_dir.exists());
}

#[test]
fn a_build_whose_copy_cannot_be_made_is_not_started_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = install(dir.path(), "app", &daemon_script(VERSION, SAYS_WHERE_IT_RAN));
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::write(&paths.daemons_dir, "not a directory").unwrap();

    let started = start_hub(&paths, &stamped(&original, 1));

    assert!(started.is_err());
    assert!(!paths.data_dir.join("started-from").exists(), "it started the original instead");
}

/// Clients of two builds can start hubs at once, and the one whose hub is up first removes the copies nothing runs from,
/// which can be the one the other was about to start. That one joins the hub that's up rather than fail.
#[test]
fn a_client_that_cannot_start_its_hub_joins_one_started_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = dir.path().join("pipe");
    assert!(Command::new("mkfifo").arg(&original).status().unwrap().success());
    let connecting = std::thread::spawn({
        let (daemon, paths) = (stamped(&original, 1), paths.clone());
        move || Client::connect_or_start(&paths, &daemon, "test").map(drop)
    });
    // It's copying, so it found no hub. Then another is up, and this copy doesn't turn out to be the build.
    let mut pipe = wait_for("the copy to open the original", || {
        OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&original).ok()
    });
    stand_in_hub::listen(&paths.socket, |_| Does::Nothing);
    pipe.write_all(daemon_script("another build", "").as_bytes()).unwrap();
    drop(pipe);

    let joined = connecting.join().unwrap();

    assert!(joined.is_ok(), "{joined:?}");
}

#[test]
fn a_daemon_that_is_not_this_build_is_not_copied() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = install(dir.path(), "app", &daemon_script("the-next-build", ""));

    let made = stamped(&original, 1).to_start(&paths);

    assert!(made.is_err());
    assert_eq!(files_in(&build_dir(&paths)), Vec::<String>::new());
}

#[test]
fn a_copy_is_kept_once_made_even_when_the_original_changes() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let first = daemon_script(VERSION, "# the first build");
    let daemon = stamped(&install(dir.path(), "app", &first), 1);
    let copy = daemon.to_start(&paths).unwrap();
    let made = std::fs::metadata(&copy).unwrap().ino();

    install(dir.path(), "app", &daemon_script("the-next-build", "# an update"));
    let again = daemon.to_start(&paths).unwrap();

    assert_eq!(again, copy);
    assert_eq!(std::fs::metadata(&copy).unwrap().ino(), made, "made again");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), first);
}

#[test]
fn a_copy_only_appears_once_it_is_whole() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let script = daemon_script(VERSION, "# from a pipe");
    let copying = CopyFromPipe::start(dir.path(), &paths, &script.as_bytes()[..4]);

    let in_place_halfway = paths.hub_copy(&build(1)).exists();
    let copy = copying.finish(&script.as_bytes()[4..]).unwrap();

    assert!(!in_place_halfway, "a half-written copy was in place");
    assert_eq!(std::fs::read_to_string(copy).unwrap(), script);
}

/// A hub can have started from the first, and on Linux, one that hasn't yet looked up its own path would find it
/// deleted.
#[test]
fn a_copy_another_client_put_in_place_first_is_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let slower = daemon_script(VERSION, "# the slower client's");
    let copying = CopyFromPipe::start(dir.path(), &paths, &slower.as_bytes()[..4]);
    let first = daemon_script(VERSION, "# the first client's");
    let copy = stamped(&install(dir.path(), "app", &first), 1).to_start(&paths).unwrap();
    let published = std::fs::metadata(&copy).unwrap().ino();

    let also = copying.finish(&slower.as_bytes()[4..]).unwrap();

    assert_eq!(also, copy);
    assert_eq!(std::fs::metadata(&copy).unwrap().ino(), published, "replaced");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), first);
    assert_eq!(files_in(&build_dir(&paths)), [DAEMON_NAME], "a temporary file was left behind");
}

/// Each writes under a temporary name of its own, so none can move another's into place, whole or not.
#[test]
fn clients_copying_at_once_each_get_a_whole_copy() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let script = daemon_script(VERSION, &"# padding\n".repeat(100_000));
    let daemon = stamped(&install(dir.path(), "app", &script), 1);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));

    let copies: Vec<_> = (0..8)
        .map(|_| {
            let (daemon, paths, barrier) = (daemon.clone(), paths.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                std::fs::read_to_string(daemon.to_start(&paths).unwrap()).unwrap()
            })
        })
        .collect();

    for copy in copies {
        assert!(copy.join().unwrap() == script, "a copy that wasn't whole");
    }
    assert_eq!(files_in(&build_dir(&paths)), [DAEMON_NAME], "a temporary file was left behind");
}

/// Files a process writes aren't quarantined unless it asks for that, so the copy runs without a check from
/// Gatekeeper, as long as the mark on the original doesn't come with it.
#[cfg(target_os = "macos")]
#[test]
fn a_quarantine_mark_on_the_original_does_not_follow_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = install(dir.path(), "app", &daemon_script(VERSION, ""));
    let xattr = |args: &[&str], file: &Path| Command::new("xattr").args(args).arg(file).output().unwrap();
    assert!(xattr(&["-w", "com.apple.quarantine", "0081;00000000;test;"], &original).status.success());

    let copy = stamped(&original, 1).to_start(&paths).unwrap();

    let mark = xattr(&["-p", "com.apple.quarantine"], &copy);
    assert!(!mark.status.success(), "{}", String::from_utf8_lossy(&mark.stdout));
}

#[test]
fn builds_with_the_same_version_and_another_number_get_copies_of_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let original = install(dir.path(), "app", &daemon_script(VERSION, ""));

    let one = stamped(&original, 1).to_start(&paths).unwrap();
    let two = stamped(&original, 2).to_start(&paths).unwrap();

    assert_ne!(one, two);
    assert_eq!(two, paths.hub_copy(&build(2)));
}

#[test]
fn a_daemon_named_by_hand_is_started_where_it_is_and_any_other_is_the_one_beside_this_binary() {
    assert_eq!(
        Daemon::find(Some("/elsewhere/roundhoused".into()), build(3).into()).unwrap(),
        Daemon::in_place("/elsewhere/roundhoused")
    );

    let beside = Daemon::find(None, build(3).into()).unwrap();

    let this_binary = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert_eq!(beside.path, this_binary.with_file_name(DAEMON_NAME));
    assert_eq!(beside.build, Some(build(3)));
}

#[test]
fn the_daemon_beside_a_linked_binary_is_in_the_version_the_link_named() {
    let dir = tempfile::tempdir().unwrap();
    let version = dir.path().canonicalize().unwrap().join("abc123");
    std::fs::create_dir(&version).unwrap();
    std::fs::write(version.join("roundhouse"), "").unwrap();
    std::os::unix::fs::symlink(&version, dir.path().join("current")).unwrap();

    let daemon = Daemon::beside(&dir.path().join("current/roundhouse"), Build::UNSTAMPED).unwrap();

    assert_eq!(daemon.path, version.join(DAEMON_NAME));
}
