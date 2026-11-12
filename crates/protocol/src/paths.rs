//! Where a hub keeps its files. Takes the environment as a function, to stay free of IO.

use std::path::{Path, PathBuf};

use crate::StampedBuild;

/// A debug build keeps its data apart from a release build's, so one run from a checkout can't reach the
/// installed app's hub, or migrate its database.
pub const DIR_NAME: &str = dir_name(cfg!(debug_assertions));
pub const ENV_HOME: &str = "ROUNDHOUSE_HOME";
pub const ENV_WORKSPACES: &str = "ROUNDHOUSE_WORKSPACES";
pub const ENV_DAEMON_BIN: &str = "ROUNDHOUSE_DAEMON_BIN";
pub const ENV_CLAUDE_BIN: &str = "ROUNDHOUSE_CLAUDE_BIN";
pub const ENV_LOG: &str = "ROUNDHOUSE_LOG";

/// The binary that runs as a hub or a worker. It gives this name in `--version` too.
pub const DAEMON_NAME: &str = "roundhoused";

/// What a worker locks in its instance's state directory while it serves the instance, so the instance has one
/// worker at a time.
pub const WORKER_LOCK_FILE: &str = "worker.lock";
/// Where a worker records itself in its instance's state directory, so its provider can tell whether one runs there.
pub const WORKER_RECORD_FILE: &str = "worker.pid";
/// The daemon's file, which on Windows has the extension a program there needs.
pub fn daemon_file() -> String {
    format!("{DAEMON_NAME}{}", std::env::consts::EXE_SUFFIX)
}

/// The variable that names the user's home directory.
pub const HOME_VAR: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// Which hub this is and what it runs. Nothing a worker starts in a workspace gets these, because the
/// code there can be another build of Roundhouse, and its commands would otherwise reach this hub.
pub const HUB_ONLY_ENV: [&str; 5] = [ENV_HOME, ENV_WORKSPACES, ENV_DAEMON_BIN, ENV_CLAUDE_BIN, ENV_LOG];

const fn dir_name(debug_build: bool) -> &'static str {
    if debug_build { "roundhouse-dev" } else { "roundhouse" }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    /// Where clients reach the hub: a Unix socket in the data directory, or on Windows a named pipe named for it.
    pub socket: PathBuf,
    pub lock: PathBuf,
    pub database: PathBuf,
    /// Copies of the database from before each migration, so an older build has one it can open.
    pub backups_dir: PathBuf,
    /// The files prompts carry, one directory per session, kept out of the database so it stays small to copy.
    pub attachments_dir: PathBuf,
    pub log: PathBuf,
    pub instances_dir: PathBuf,
    /// What each workspace's worker writes to stderr, a file per instance, whichever provider runs it.
    pub worker_logs_dir: PathBuf,
    /// The providers this hub runs workspaces on, besides the local one. *Not built:* naming them in the hub's settings.
    pub providers: PathBuf,
    /// A copy of the daemon for each stamped build that has started a hub here. See [`Paths::hub_copy`].
    pub daemons_dir: PathBuf,
    pub workspaces_root: PathBuf,
}

impl Paths {
    /// The copy of the daemon that `build`'s hubs run from, in a directory named for the build.
    pub fn hub_copy(&self, build: &StampedBuild) -> PathBuf {
        self.daemons_dir.join(format!("{}-{}", build.version(), build.number())).join(daemon_file())
    }
}

/// macOS limits a socket path to 103 bytes.
pub const MAX_SOCKET_PATH_BYTES: usize = 103;

pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Result<Paths, String> {
    let home = env(HOME_VAR).filter(|h| !h.is_empty()).map(PathBuf::from);
    let no_home = || format!("{HOME_VAR} is not set");
    let data_dir = if let Some(dir) = env(ENV_HOME).filter(|v| !v.is_empty()) {
        PathBuf::from(dir)
    } else if cfg!(windows) {
        let local = env("LOCALAPPDATA").filter(|v| !v.is_empty()).ok_or("LOCALAPPDATA is not set")?;
        // Not the bare name. Velopack installs an app in a directory named for it there, with its updater and
        // packages, and Windows would take that one for this, since it ignores case.
        PathBuf::from(local).join(format!("{DIR_NAME}-data"))
    } else {
        let home = home.clone().ok_or_else(no_home)?;
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support").join(DIR_NAME)
        } else {
            match env("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
                Some(dir) => PathBuf::from(dir).join(DIR_NAME),
                None => home.join(".local/share").join(DIR_NAME),
            }
        }
    };
    let workspaces_root = match env(ENV_WORKSPACES).filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home.ok_or_else(no_home)?.join(DIR_NAME).join("workspaces"),
    };
    if cfg!(windows) {
        // A hub that a client starts runs in its data directory there, not where the client is, so a relative path
        // would name a different directory to each. The pipe is also named for the path as it's spelled.
        absolute(&data_dir)?;
        absolute(&workspaces_root)?;
    }
    let socket = if cfg!(windows) { pipe(&data_dir) } else { socket_in(&data_dir)? };
    Ok(Paths {
        socket,
        lock: data_dir.join("hub.lock"),
        database: data_dir.join("hub.db"),
        backups_dir: data_dir.join("backups"),
        attachments_dir: data_dir.join("attachments"),
        log: data_dir.join("hub.log"),
        instances_dir: data_dir.join("instances"),
        worker_logs_dir: data_dir.join("worker-logs"),
        providers: data_dir.join("providers.json"),
        daemons_dir: data_dir.join("daemons"),
        workspaces_root,
        data_dir,
    })
}

fn absolute(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Ok(());
    }
    Err(format!(
        "{} isn't a whole path, from the drive down, which Windows needs. Set {ENV_HOME} and {ENV_WORKSPACES} to ones \
         that are",
        path.display()
    ))
}

fn socket_in(data_dir: &Path) -> Result<PathBuf, String> {
    let socket = data_dir.join("hub.sock");
    if socket.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
        return Err(format!(
            "socket path {} is longer than the {MAX_SOCKET_PATH_BYTES} bytes the OS allows. Set {ENV_HOME} to a shorter path",
            socket.display()
        ));
    }
    Ok(socket)
}

/// The named pipe of the hub that keeps its files in `data_dir`. A pipe has no place in a directory, so its name is
/// made from the directory's, one pipe for each as there's one socket in each elsewhere, and a checkout's hub is
/// still out of an installed app's reach. Windows ignores case in a path and takes either slash, so those don't
/// change the name. Any other two spellings of one directory do, such as one that goes through `..`.
fn pipe(data_dir: &Path) -> PathBuf {
    let spelled = data_dir.to_string_lossy().to_lowercase().replace('/', "\\");
    PathBuf::from(format!(r"\\.\pipe\{DIR_NAME}-{:016x}", fnv1a(spelled.trim_end_matches('\\').as_bytes())))
}

/// 64-bit FNV-1a. Written out, because a client only finds a hub from another build if both make the same name, and
/// std doesn't promise its own hash stays the same from one release of Rust to the next.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    /// A whole path on the platform the test runs on.
    fn whole(path: &str) -> String {
        if cfg!(windows) { format!("C:{path}") } else { path.to_string() }
    }

    #[test]
    fn overrides_win_over_defaults() {
        let (home, data, workspaces) = (whole("/h"), whole("/d"), whole("/w"));

        let paths = resolve(env_of(&[(HOME_VAR, &home), (ENV_HOME, &data), (ENV_WORKSPACES, &workspaces)])).unwrap();

        let socket = if cfg!(windows) { pipe(Path::new(&data)) } else { PathBuf::from("/d/hub.sock") };
        assert_eq!(paths.data_dir, PathBuf::from(data));
        assert_eq!(paths.socket, socket);
        assert_eq!(paths.workspaces_root, PathBuf::from(workspaces));
    }

    #[test]
    fn workspaces_default_to_a_visible_folder_in_home() {
        let (home, local) = (whole("/h"), whole("/l"));

        let paths = resolve(env_of(&[(HOME_VAR, &home), ("LOCALAPPDATA", &local)])).unwrap();

        assert_eq!(paths.workspaces_root, Path::new(&home).join(DIR_NAME).join("workspaces"));
    }

    #[test]
    fn only_a_whole_path_counts_as_absolute() {
        assert_eq!(absolute(Path::new(&whole("/d"))), Ok(()));
        assert!(absolute(Path::new("d")).is_err());
        assert!(absolute(Path::new("../d")).is_err());
    }

    /// On Windows, where a hub a client starts doesn't run where the client does.
    #[cfg(windows)]
    #[test]
    fn a_relative_data_directory_or_workspaces_folder_is_refused() {
        let (home, data) = (whole("/h"), whole("/d"));

        assert!(resolve(env_of(&[(HOME_VAR, &home), (ENV_HOME, "d")])).is_err());
        assert!(resolve(env_of(&[(HOME_VAR, &home), (ENV_HOME, &data), (ENV_WORKSPACES, "w")])).is_err());
    }

    #[test]
    fn a_pipe_is_named_for_its_data_directory_however_the_directory_is_spelled() {
        let named = pipe(Path::new(r"C:\Users\Someone\AppData\Local\roundhouse-data"));

        // Pinned, since a build that named it differently couldn't find a hub that another build started.
        assert_eq!(named, PathBuf::from(format!(r"\\.\pipe\{DIR_NAME}-aa12b68e414427bf")));
        assert_eq!(pipe(Path::new("c:/users/someone/appdata/local/roundhouse-data/")), named);
        assert_ne!(pipe(Path::new(r"C:\Users\Someone\AppData\Local\roundhouse-dev-data")), named);
    }

    #[test]
    fn the_hash_is_fnv1a() {
        // The reference values for it.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn only_a_release_build_defaults_to_the_real_data() {
        assert_eq!(dir_name(false), "roundhouse");
        assert_eq!(dir_name(true), "roundhouse-dev");
        assert_eq!(DIR_NAME, dir_name(cfg!(debug_assertions)));
    }

    #[test]
    fn a_hub_copy_is_named_for_its_build_version_and_number() {
        let (home, data) = (whole("/h"), whole("/d"));
        let paths = resolve(env_of(&[(HOME_VAR, &home), (ENV_HOME, &data)])).unwrap();

        let copy = paths.hub_copy(&StampedBuild::new("abc123", 7).unwrap());

        assert_eq!(copy, Path::new(&data).join("daemons/abc123-7").join(daemon_file()));
    }

    /// Only where the hub is on a socket, which has a path.
    #[cfg(unix)]
    #[test]
    fn a_socket_path_the_os_would_reject_is_an_error_up_front() {
        let long = format!("/{}", "x".repeat(120));
        assert!(resolve(env_of(&[("HOME", "/h"), (ENV_HOME, &long)])).is_err());
    }
}
