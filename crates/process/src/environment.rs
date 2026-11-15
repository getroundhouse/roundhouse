use std::ffi::OsString;
use std::process::Command;

/// What a process is given beyond the environment it inherits: variables set, and variables taken out. It goes on a
/// command last, just before it starts, so nothing set on the command before can undo it. A `tokio` command takes it
/// through `as_std_mut`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    set: Vec<(OsString, OsString)>,
    taken_out: Vec<OsString>,
}

impl Environment {
    /// Nothing beyond what a process inherits.
    pub const NONE: Self = Self { set: Vec::new(), taken_out: Vec::new() };

    /// Gives `name` as `value`.
    #[must_use]
    pub fn with(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.set.push((name.into(), value.into()));
        self
    }

    /// Takes `name` out of what a process inherits.
    #[must_use]
    pub fn without(mut self, name: impl Into<OsString>) -> Self {
        self.taken_out.push(name.into());
        self
    }

    /// Puts it on `command`.
    pub fn apply(&self, command: &mut Command) {
        for name in &self.taken_out {
            command.env_remove(name);
        }
        for (name, value) in &self.set {
            command.env(name, value);
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

    #[test]
    fn none_leaves_a_command_as_it_was() {
        let mut command = Command::new("git");
        command.env("GH_TOKEN", "the-machines-own");

        Environment::NONE.apply(&mut command);

        assert_eq!(command.get_envs().count(), 1);
        assert_eq!(Environment::NONE, Environment::default());
    }
}
