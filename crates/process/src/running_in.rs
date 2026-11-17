//! Which processes run from a program inside a folder, such as a build made there. On Windows a file in a folder that a
//! process has open keeps the folder from being moved, and one run from the folder is the likeliest to have one, as a
//! hub built there has its database, so whatever couldn't move it can say which to stop first. Running from the folder
//! doesn't hold it by itself.

use std::path::{Path, PathBuf};

use crate::sys;

/// A process, and the program it runs from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningProgram {
    pub pid: u32,
    pub program: PathBuf,
}

/// Every process this user can see that runs from a program inside `folder`.
///
/// On Unix it finds none, since nothing a process has open keeps a folder from being moved there, so there's never one
/// to name. On Windows a process that has a file in the folder open but runs from elsewhere, such as one started by
/// a program in it, holds it too, and so does one whose current folder is in it, and neither is found.
pub fn running_in(folder: &Path) -> Vec<RunningProgram> {
    sys::running_programs().into_iter().filter(|running| inside(&running.program, folder)).collect()
}

/// Whether `path` is in `folder`, or below it. Windows doesn't tell paths apart by case.
fn inside(path: &Path, folder: &Path) -> bool {
    if cfg!(windows) {
        let lower = |path: &Path| PathBuf::from(path.to_string_lossy().to_lowercase());
        lower(path).starts_with(lower(folder))
    } else {
        path.starts_with(folder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_path_below_the_folder_is_inside_it() {
        let folder = std::env::temp_dir().join("ws").join("alder");

        assert!(inside(&folder.join("target").join("hub"), &folder));
        assert!(!inside(&folder.with_file_name("alder-2").join("hub"), &folder), "a name that only starts the same");
        assert!(!inside(&folder.with_file_name("birch").join("hub"), &folder));
    }

    /// A copy of a program Windows has, run from a folder of its own, as a build made in a worktree is run.
    #[cfg(windows)]
    #[test]
    fn a_process_running_from_a_program_in_the_folder_is_found() {
        let folder = tempfile::tempdir().unwrap();
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let program = folder.path().join("PING.EXE");
        std::fs::copy(system.join("PING.EXE"), &program).unwrap();
        let mut running = std::process::Command::new(&program)
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();

        let found = running_in(folder.path());
        let beside = running_in(&system.join("drivers"));

        running.kill().unwrap();
        running.wait().unwrap();
        assert!(found.iter().any(|it| it.pid == running.id() && it.program.ends_with("PING.EXE")), "{found:?}");
        assert!(beside.iter().all(|it| it.pid != running.id()), "{beside:?}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_tells_no_paths_apart_by_case() {
        assert!(inside(Path::new(r"C:\WS\Alder\target\hub.exe"), Path::new(r"c:\ws\alder")));
        assert!(inside(Path::new(r"C:\ws\alder\target\hub.exe"), Path::new("C:/ws/alder")));
    }
}
