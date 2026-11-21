//! Unpacking the archive of the release a hub updates to, and having that release's build place itself beside the one
//! the service runs, and point the service at it, as `roundhouse hub install` has a build do.

use std::fs::File;
use std::io;
use std::path::{Component, Path};
use std::process::{Command, Stdio};

use protocol::paths::{cli_file, daemon_file, daemon_version_line, service_programs};

/// Why a release couldn't be placed.
#[derive(Debug, thiserror::Error)]
pub(super) enum PlaceError {
    #[error("couldn't unpack the archive: {0}")]
    Unpack(io::Error),
    #[error("the archive doesn't hold {0}")]
    Missing(String),
    #[error("the archive holds {name} as something other than a file")]
    NotAFile { name: String },
    #[error("couldn't run the release's {program}: {source}")]
    Run { program: String, source: io::Error },
    #[error("the release's daemon isn't that release: {0}")]
    OtherBuild(io::Error),
    #[error("the release couldn't place itself: {0}")]
    NotPlaced(String),
}

/// Unpacks `archive` into `into`, checks that its daemon is `version`'s, and has its command line place that build
/// where the service runs it, which points the service at it. `into` is made afresh, and holds only the service's
/// programs, whatever else the archive has.
pub(super) fn place(archive: &Path, version: &str, into: &Path) -> Result<(), PlaceError> {
    unpack(archive, into)?;
    process::check_version(&into.join(daemon_file()), &daemon_version_line(version)).map_err(PlaceError::OtherBuild)?;
    // It finds where to place itself from the environment, which is this hub's, as the service gave it.
    output_of(Command::new(into.join(cli_file())).args(["hub", "place"]), &cli_file())?;
    Ok(())
}

/// What `command` printed, once it has run and succeeded, with nothing for its input.
fn output_of(command: &mut Command, program: &str) -> Result<String, PlaceError> {
    process::without_window(command);
    // Each was just unpacked.
    let ran = process::run_once_not_busy(|| command.stdin(Stdio::null()).output());
    let output = ran.map_err(|source| PlaceError::Run { program: program.to_owned(), source })?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(PlaceError::NotPlaced(format!("{program} {}: {said}", output.status)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Unpacks each of the service's programs in `archive` into `into`, which it makes afresh, as a file only this user
/// can open or run.
fn unpack(archive: &Path, into: &Path) -> Result<(), PlaceError> {
    let _ = std::fs::remove_dir_all(into);
    private_files::create_dir_all(into).map_err(PlaceError::Unpack)?;
    let file = File::open(archive).map_err(PlaceError::Unpack)?;
    let mut entries = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let programs = service_programs();
    let mut unpacked = Vec::new();
    for entry in entries.entries().map_err(PlaceError::Unpack)? {
        let mut entry = entry.map_err(PlaceError::Unpack)?;
        let path = entry.path().map_err(PlaceError::Unpack)?.into_owned();
        // As `tar` from inside the folder writes it, with or without a `./` first.
        let mut parts = path.components().filter(|part| *part != Component::CurDir);
        let (Some(Component::Normal(name)), None) = (parts.next(), parts.next()) else { continue };
        let Some(program) = programs.iter().find(|program| name == program.as_str()) else { continue };
        if !entry.header().entry_type().is_file() {
            return Err(PlaceError::NotAFile { name: program.clone() });
        }
        let mut out = executable(&into.join(program)).map_err(PlaceError::Unpack)?;
        io::copy(&mut entry, &mut out).map_err(PlaceError::Unpack)?;
        unpacked.push(program.clone());
    }
    match programs.into_iter().find(|program| !unpacked.contains(program)) {
        Some(missing) => Err(PlaceError::Missing(missing)),
        None => Ok(()),
    }
}

/// A new file at `path` that this user can run.
fn executable(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o700);
    options.open(path)
}
