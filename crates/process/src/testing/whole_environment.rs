//! The whole environment of a process a test starts, which is only what the test gives it.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

use crate::Environment;
use protocol::testing::ENV_TEST_PROCESS;

/// What a process a test starts keeps of the test's own environment, by name: where to find the programs it runs,
/// where to keep temporary files, and on Windows what any process there needs to start. Nothing else, so nothing of
/// the person's or the runner's login, settings or tools reaches it, such as a token for GitHub, a `GIT_DIR` that a
/// git hook sets, or a variable a coding agent sets for the commands it runs.
const KEPT: &[&str] =
    if cfg!(windows) { &["PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "TEMP", "TMP"] } else { &["PATH", "TMPDIR"] };

/// Where a process finds its user's home.
const HOME: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// A login shell that prints nothing and fails, so a hub, which asks the login shell for the environment everything it
/// starts gets, gives each its own, and none of the person's shell setup runs.
const SAYS_NOTHING: &str = "/usr/bin/false";

/// The whole environment of a process a test starts, in place of the test's own, as against a
/// [`Environment`], which is what a process gets beyond what it inherits. Every process a test starts that
/// reads its environment, such as a hub or a worker, runs in one, so what's in the environment of whoever runs the
/// tests can't change what they do, and every variable it reads is one the test chose.
#[derive(Clone, Debug)]
pub struct WholeEnvironment {
    vars: BTreeMap<OsString, OsString>,
}

impl WholeEnvironment {
    /// Only what [`KEPT`] names of this process's environment, with `home` as the user's home, which needn't exist, so
    /// none of the person's settings there are read, a login shell that says nothing, and this process as the test, so
    /// a hub started in it stops once the test has gone, even one that's killed before it can stop the hub.
    pub fn for_a_test(home: &Path) -> Self {
        let mut environment = Self::kept();
        environment.set(HOME, home).set("SHELL", SAYS_NOTHING).set(ENV_TEST_PROCESS, std::process::id().to_string());
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

    /// What a command that inherits this process's environment is given to start in this one instead, for code under
    /// test that starts its own processes and takes an [`Environment`] for them, as the worker's git does: nothing of
    /// what it inherits, and each of this one's variables. What the code told the command to set before it applies
    /// this, such as a setting of its own or a script's variables, stays, unless this gives it too.
    pub fn given(&self) -> Environment {
        self.vars().fold(Environment::INHERITING_NOTHING, |given, (name, value)| given.with(name, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cargo sets `CARGO_MANIFEST_DIR` for a test, and it's in none of the names kept.
    #[test]
    fn a_test_s_environment_keeps_only_what_finds_programs_and_temporary_files_and_names_the_test() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
        let home = Path::new("nowhere");

        let environment = WholeEnvironment::for_a_test(home);

        let names: Vec<&OsStr> = environment.vars().map(|(name, _)| name).collect();
        let given = [HOME, "SHELL", ENV_TEST_PROCESS];
        assert!(names.iter().all(|name| KEPT.iter().chain(&given).any(|kept| name == kept)), "{names:?}");
        assert_eq!(environment.get("PATH"), std::env::var_os("PATH").as_deref());
        assert_eq!(environment.get(HOME), Some(home.as_os_str()));
        assert_eq!(environment.get("SHELL"), Some(OsStr::new(SAYS_NOTHING)));
        assert_eq!(environment.get(ENV_TEST_PROCESS), Some(OsStr::new(&std::process::id().to_string())));
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

    /// The names of the variables `env` printed that it ran with, but for what the system gives every process.
    #[cfg(unix)]
    fn names_printed(env: &mut Command) -> Vec<String> {
        let printed = String::from_utf8(env.output().unwrap().stdout).unwrap();
        let names = printed.lines().filter_map(|line| line.split_once('=')).map(|(name, _)| name.to_string());
        let mut names: Vec<String> = names.filter(|name| name != THE_SYSTEM_GIVES).collect();
        names.sort_unstable();
        names
    }

    /// What [`KEPT`] names that this process has, and `also`.
    #[cfg(unix)]
    fn kept_and(also: &[&str]) -> Vec<String> {
        let kept = KEPT.iter().copied().filter(|name| std::env::var_os(name).is_some());
        let mut expected: Vec<String> = kept.chain(also.iter().copied()).map(str::to_string).collect();
        expected.sort_unstable();
        expected
    }

    /// Run, so it's the process's environment that's checked, rather than what the command was told to set.
    #[cfg(unix)]
    #[test]
    fn a_process_started_in_it_has_nothing_else() {
        let mut environment = WholeEnvironment::kept();
        environment.set("ONLY", "this");

        let names = names_printed(&mut environment.command("env"));

        assert_eq!(names, kept_and(&["ONLY"]));
    }

    /// As code under test starts a process, inheriting this one's environment, which has what Cargo sets for a test,
    /// and telling it to set a variable of its own, which this process has too, as it would a script's.
    #[cfg(unix)]
    #[test]
    fn a_process_given_it_has_nothing_else_but_what_its_command_was_told() {
        let has = |name| std::env::var_os(name).is_some();
        assert!(has("CARGO_MANIFEST_DIR") && has("CARGO_PKG_NAME"), "Cargo sets them for a test");
        let mut environment = WholeEnvironment::kept();
        environment.set("ONLY", "this");
        let mut env = Command::new("env");
        env.env("CARGO_PKG_NAME", "told");

        environment.given().apply(&mut env);

        assert_eq!(names_printed(&mut env), kept_and(&["CARGO_PKG_NAME", "ONLY"]));
    }
}
