//! The whole environment of a process a test starts, which is only what the test gives it.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

/// What a process a test starts keeps of the test's own environment, by name: where to find the programs it runs,
/// where to keep temporary files, and on Windows what any process there needs to start. Nothing else, so nothing of
/// the person's or the runner's login, settings or tools reaches it, such as a token for GitHub, a `GIT_DIR` that a
/// git hook sets, or a variable a coding agent sets for the commands it runs.
const KEPT: &[&str] =
    if cfg!(windows) { &["PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "TEMP", "TMP"] } else { &["PATH", "TMPDIR"] };

/// Where a process finds its user's home.
const HOME: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// A login shell that prints nothing and fails, so a hub that asks the login shell for the `PATH` its workers get, as
/// the local provider does, gives them its own, and none of the person's shell setup runs.
const SAYS_NOTHING: &str = "/usr/bin/false";

/// The whole environment of a process a test starts, in place of the test's own, as against a
/// [`crate::Environment`], which is what a process gets beyond what it inherits. Every process a test starts that
/// reads its environment, such as a hub or a worker, runs in one, so what's in the environment of whoever runs the
/// tests can't change what they do, and every variable it reads is one the test chose.
#[derive(Clone, Debug)]
pub struct WholeEnvironment {
    vars: BTreeMap<OsString, OsString>,
}

impl WholeEnvironment {
    /// Only what [`KEPT`] names of this process's environment, with `home` as the user's home, which needn't exist, so
    /// none of the person's settings there are read, and a login shell that says nothing.
    pub fn for_a_test(home: &Path) -> Self {
        let mut environment = Self::kept();
        environment.set(HOME, home).set("SHELL", SAYS_NOTHING);
        environment
    }

    /// Only what [`KEPT`] names of this process's environment.
    pub(super) fn kept() -> Self {
        let vars = std::env::vars_os().filter(|(name, _)| KEPT.iter().any(|kept| name == kept)).collect();
        Self { vars }
    }

    /// Gives `name` as `value`, in place of any value it had.
    pub fn set(&mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> &mut Self {
        self.vars.insert(name.into(), value.into());
        self
    }

    /// Leaves `name` out.
    pub fn remove(&mut self, name: impl AsRef<OsStr>) -> &mut Self {
        self.vars.remove(name.as_ref());
        self
    }

    /// Puts `dir` at the front of `PATH`, so a program there, such as a stand-in, is found before any other.
    ///
    /// # Panics
    ///
    /// If `dir` can't be in a `PATH`, as one with the separator in it can't.
    pub fn put_first_on_path(&mut self, dir: &Path) -> &mut Self {
        let path = self.vars.get(OsStr::new("PATH")).map(std::env::split_paths).into_iter().flatten();
        let joined = std::env::join_paths(std::iter::once(dir.to_path_buf()).chain(path))
            .unwrap_or_else(|e| panic!("{} can't go in a PATH: {e}", dir.display()));
        self.set("PATH", joined)
    }

    /// What it gives `name`.
    pub fn get(&self, name: impl AsRef<OsStr>) -> Option<&OsStr> {
        self.vars.get(name.as_ref()).map(OsString::as_os_str)
    }

    /// Each variable, by name.
    pub fn vars(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.vars.iter().map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }

    /// `program`, to start in this environment and no other.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear().envs(&self.vars);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cargo sets `CARGO_MANIFEST_DIR` for a test, and it's in none of the names kept.
    #[test]
    fn a_test_s_environment_keeps_only_what_finds_programs_and_temporary_files() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
        let home = Path::new("nowhere");

        let environment = WholeEnvironment::for_a_test(home);

        let names: Vec<&OsStr> = environment.vars().map(|(name, _)| name).collect();
        assert!(names.iter().all(|name| KEPT.iter().chain(&[HOME, "SHELL"]).any(|kept| name == kept)), "{names:?}");
        assert_eq!(environment.get("PATH"), std::env::var_os("PATH").as_deref());
        assert_eq!(environment.get(HOME), Some(home.as_os_str()));
        assert_eq!(environment.get("SHELL"), Some(OsStr::new(SAYS_NOTHING)));
    }

    #[test]
    fn a_directory_put_first_on_the_path_comes_before_the_rest() {
        let mut environment = WholeEnvironment::kept();
        environment.set("PATH", std::env::join_paths(["one", "two"]).unwrap());

        environment.put_first_on_path(Path::new("stand-ins"));

        let path: Vec<_> = std::env::split_paths(environment.get("PATH").unwrap()).collect();
        assert_eq!(path, [Path::new("stand-ins"), Path::new("one"), Path::new("two")]);
    }

    /// What macOS gives every process, even one started with nothing.
    #[cfg(unix)]
    const THE_SYSTEM_GIVES: &str = "__CF_USER_TEXT_ENCODING";

    /// Run, so it's the process's environment that's checked, rather than what the command was told to set.
    #[cfg(unix)]
    #[test]
    fn a_process_started_in_it_has_nothing_else() {
        let mut environment = WholeEnvironment::kept();
        environment.set("ONLY", "this");

        let printed = environment.command("env").output().unwrap();

        let printed = String::from_utf8(printed.stdout).unwrap();
        let names = printed.lines().filter_map(|line| line.split_once('=')).map(|(name, _)| name);
        let mut names: Vec<&str> = names.filter(|name| *name != THE_SYSTEM_GIVES).collect();
        names.sort_unstable();
        let mut expected: Vec<&str> = KEPT.iter().copied().filter(|name| std::env::var_os(name).is_some()).collect();
        expected.push("ONLY");
        expected.sort_unstable();
        assert_eq!(names, expected, "{printed}");
    }
}
