use std::ffi::{OsStr, OsString};
use std::process::Command;

/// What a process is given beyond the environment it inherits: variables set, and variables taken out, or else a whole
/// environment in place of the one it inherits. It goes on a command last, just before it starts, so nothing set on the
/// command before can undo it. A `tokio` command takes it through `as_std_mut`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    set: Vec<(OsString, OsString)>,
    taken_out: Vec<OsString>,
    /// Whether a process keeps nothing of what it inherits, but what its command was told to set, as the hub gives what
    /// it starts its machine's environment, and a test the code under it, through `testing::WholeEnvironment::given`.
    inherits_nothing: bool,
}

impl Environment {
    /// Nothing beyond what a process inherits.
    pub const NONE: Self = Self { set: Vec::new(), taken_out: Vec::new(), inherits_nothing: false };

    /// Nothing of what a process inherits, but what its command was told to set, such as a setting of the code that
    /// starts it, which stays as the code set it, and what this gives it after.
    pub const INHERITING_NOTHING: Self = Self { set: Vec::new(), taken_out: Vec::new(), inherits_nothing: true };

    /// `vars` alone, in place of what a process inherits, as [`Environment::INHERITING_NOTHING`] says.
    #[must_use]
    pub fn whole<N: Into<OsString>, V: Into<OsString>>(vars: impl IntoIterator<Item = (N, V)>) -> Self {
        vars.into_iter().fold(Self::INHERITING_NOTHING, |whole, (name, value)| whole.with(name, value))
    }

    /// Gives `name` as `value`.
    #[must_use]
    pub fn with(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.set.push((name.into(), value.into()));
        self
    }

    /// Takes `name` out of what a process inherits, and out of what this gave it before, so the process has none of
    /// it. Giving it after gives it again.
    #[must_use]
    pub fn without(mut self, name: impl Into<OsString>) -> Self {
        let name = name.into();
        self.set.retain(|(given, _)| *given != name);
        self.taken_out.push(name);
        self
    }

    /// Puts it on `command`.
    pub fn apply(&self, command: &mut impl TakesEnvironment) {
        if self.inherits_nothing {
            command.inherit_nothing();
        }
        for name in &self.taken_out {
            command.take_out(name);
        }
        for (name, value) in &self.set {
            command.give(name, value);
        }
    }
}

/// Whether `a` and `b` name the same variable, which on Windows doesn't depend on their case, so taking out one a
/// command inherits doesn't take out one it was told to set.
fn same(a: &OsStr, b: &OsStr) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// A command an [`Environment`] goes on, which starts its process with what it inherits and these changes to it: a
/// standard library's, or another, such as the one a PTY's program is started with.
pub trait TakesEnvironment {
    fn take_out(&mut self, name: &OsStr);
    fn give(&mut self, name: &OsStr, value: &OsStr);

    /// Takes out everything the process would inherit, but what the command was told to set.
    fn inherit_nothing(&mut self);
}

impl TakesEnvironment for Command {
    fn take_out(&mut self, name: &OsStr) {
        self.env_remove(name);
    }

    fn give(&mut self, name: &OsStr, value: &OsStr) {
        self.env(name, value);
    }

    /// What it inherits is this process's environment, as it is when it starts. Each is taken out by name, so the
    /// command lists what it doesn't have, as code that checks a command for a variable it mustn't pass on reads it.
    fn inherit_nothing(&mut self) {
        let told: Vec<OsString> = self.get_envs().map(|(name, _)| name.to_os_string()).collect();
        let inherited =
            std::env::vars_os().map(|(name, _)| name).filter(|name| !told.iter().any(|told| same(told, name)));
        for name in inherited.collect::<Vec<_>>() {
            self.env_remove(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    /// What `command` changes of what it inherits, by name, with no value for what it takes out.
    fn changes(command: &Command) -> Vec<(&OsStr, Option<&OsStr>)> {
        let mut changes: Vec<_> = command.get_envs().collect();
        changes.sort();
        changes
    }

    #[test]
    fn what_it_gives_and_takes_out_goes_on_last() {
        let mut command = Command::new("git");
        command.env("GH_TOKEN", "the-machines-own").env("KEPT", "kept");
        let environment = Environment::NONE.with("GH_CONFIG_DIR", "/gh").without("GH_TOKEN");

        environment.apply(&mut command);

        let os = OsStr::new;
        let taken_out = None;
        assert_eq!(
            changes(&command),
            [(os("GH_CONFIG_DIR"), Some(os("/gh"))), (os("GH_TOKEN"), taken_out), (os("KEPT"), Some(os("kept")))]
        );
    }

    /// As code that adds to an environment it was handed takes out what that one gave, such as the worker taking git's
    /// repository variables out of the one it was built with.
    #[test]
    fn what_it_takes_out_it_no_longer_gives_until_it_gives_it_again() {
        let environment = Environment::NONE.with("GIT_DIR", "/elsewhere").with("KEPT", "kept").without("GIT_DIR");
        let given_again = environment.clone().with("GIT_DIR", "/here");
        let (mut command, mut again) = (Command::new("git"), Command::new("git"));

        environment.apply(&mut command);
        given_again.apply(&mut again);

        let os = OsStr::new;
        let taken_out = None;
        assert_eq!(changes(&command), [(os("GIT_DIR"), taken_out), (os("KEPT"), Some(os("kept")))]);
        assert_eq!(changes(&again), [(os("GIT_DIR"), Some(os("/here"))), (os("KEPT"), Some(os("kept")))]);
    }

    #[test]
    fn none_leaves_a_command_as_it_was() {
        let mut command = Command::new("git");
        command.env("GH_TOKEN", "the-machines-own");

        Environment::NONE.apply(&mut command);

        assert_eq!(command.get_envs().count(), 1);
        assert_eq!(Environment::NONE, Environment::default());
    }
}
