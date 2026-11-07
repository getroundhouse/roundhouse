//! Where a hub keeps its files. Takes the environment as a function, to stay free of IO.

use std::path::PathBuf;

pub const DIR_NAME: &str = "trestle";
pub const ENV_HOME: &str = "TRESTLE_HOME";
pub const ENV_WORKSPACES: &str = "TRESTLE_WORKSPACES";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub socket: PathBuf,
    pub lock: PathBuf,
    pub database: PathBuf,
    pub log: PathBuf,
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
}

/// macOS limits a socket path to 103 bytes.
pub const MAX_SOCKET_PATH_BYTES: usize = 103;

pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Result<Paths, String> {
    let home = env("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    let data_dir = if let Some(dir) = env(ENV_HOME).filter(|v| !v.is_empty()) {
        PathBuf::from(dir)
    } else {
        let home = home.clone().ok_or("HOME is not set")?;
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
        None => home.ok_or("HOME is not set")?.join(DIR_NAME).join("workspaces"),
    };
    let socket = data_dir.join("hub.sock");
    if socket.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
        return Err(format!(
            "socket path {} is longer than the {MAX_SOCKET_PATH_BYTES} bytes the OS allows. Set {ENV_HOME} to a shorter path",
            socket.display()
        ));
    }
    Ok(Paths {
        socket,
        lock: data_dir.join("hub.lock"),
        database: data_dir.join("hub.db"),
        log: data_dir.join("hub.log"),
        instances_dir: data_dir.join("instances"),
        workspaces_root,
        data_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn overrides_win_over_defaults() {
        let paths = resolve(env_of(&[("HOME", "/h"), (ENV_HOME, "/d"), (ENV_WORKSPACES, "/w")])).unwrap();
        assert_eq!(paths.socket, PathBuf::from("/d/hub.sock"));
        assert_eq!(paths.workspaces_root, PathBuf::from("/w"));
    }

    #[test]
    fn workspaces_default_to_a_visible_folder_in_home() {
        let paths = resolve(env_of(&[("HOME", "/h")])).unwrap();
        assert_eq!(paths.workspaces_root, PathBuf::from("/h/trestle/workspaces"));
    }

    #[test]
    fn a_socket_path_the_os_would_reject_is_an_error_up_front() {
        let long = format!("/{}", "x".repeat(120));
        assert!(resolve(env_of(&[("HOME", "/h"), (ENV_HOME, &long)])).is_err());
    }
}
