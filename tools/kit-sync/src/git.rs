//! Runs git: in the tool's own repositories without the person's settings, and in theirs with them.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::process::{Command, Output, Stdio};

use std::path::Path;

use crate::error::Error;

/// The file that's always empty, which git reads in place of the person's settings, attributes and ignore files.
const NOTHING: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// The variables that say where a repository is, which git would take over the folder it's run in.
const WHERE: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
];

/// Whose settings git reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Settings {
    /// None of the person's, in the tool's own repositories: no config, attributes, ignore files, templates or any
    /// other variable of git's, since any could change what git does there, such as merging only by fast-forward,
    /// writing diffs without prefixes, running a hook, ignoring a file or taking one as binary.
    Isolated,
    /// The person's, for reaching upstream, which they say how to reach, and for reading this repository, whose
    /// ignore files they are.
    Persons,
}

/// Runs git with an environment of its own, such as the process's, as `main` gives it.
#[derive(Clone, Debug)]
pub struct Git {
    environment: BTreeMap<OsString, OsString>,
}

impl Git {
    pub fn new(environment: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        Self { environment: environment.into_iter().collect() }
    }

    /// What git printed, run in `dir` with `args`, or why it failed.
    pub(crate) fn run<A: AsRef<OsStr>>(
        &self,
        settings: Settings,
        dir: &Path,
        args: impl IntoIterator<Item = A>,
    ) -> Result<String, Error> {
        let args: Vec<OsString> = args.into_iter().map(|arg| arg.as_ref().to_os_string()).collect();
        let output = self.output(settings, dir, &args, None)?;
        if !output.status.success() {
            return Err(Error::Git {
                command: args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>().join(" "),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// What git did, run in `dir` with `args` and `input` to read, whether it failed or not.
    pub(crate) fn output(
        &self,
        settings: Settings,
        dir: &Path,
        args: &[OsString],
        input: Option<File>,
    ) -> Result<Output, Error> {
        let mut command = Command::new("git");
        command.current_dir(dir).env_clear().envs(self.environment.iter().filter(|(name, _)| {
            let name = name.to_string_lossy();
            match settings {
                Settings::Isolated => !name.starts_with("GIT_"),
                Settings::Persons => !WHERE.contains(&name.as_ref()),
            }
        }));
        if settings == Settings::Isolated {
            command.env("GIT_CONFIG_GLOBAL", NOTHING).env("GIT_CONFIG_NOSYSTEM", "1");
            // Each read from a place of its own, such as ~/.config/git, whatever the settings say.
            command.arg("-c").arg(format!("core.attributesFile={NOTHING}"));
            command.arg("-c").arg(format!("core.excludesFile={NOTHING}"));
        }
        command.args(["-c", "user.name=kit-sync", "-c", "user.email=kit-sync@localhost"]).args(args);
        command.stdin(input.map_or_else(Stdio::null, Stdio::from));
        command.output().map_err(Error::NoGit)
    }
}
