use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use process::Environment;
use tokio::sync::OnceCell;
use tracing::{debug, warn};

use super::Vars;
use super::kept::{Started, same_name};
use super::profile;

/// The whole environment of everything the hub starts on its machine: a local worker, and so its agents, scripts, git
/// and terminals, a provider's command and adapter, `gh` and `ssh`. It's the user's profile's, as their login shell
/// has it, with the OS session's few variables that a profile doesn't set and the hub's own, as [`Started`] keeps
/// them, so it's the same whoever started the hub: an app opened from the Dock, which has none of the profile's, or a
/// terminal inside a Claude session, whose variables would reach a Claude CLI the hub starts. It's read once, and
/// clones share the read.
#[derive(Clone)]
pub(crate) struct MachineEnvironment {
    started: Arc<Started>,
    /// The daemon, which the login shell runs to print what it has.
    printer: Arc<Path>,
    whole: Arc<OnceCell<Vars>>,
}

impl MachineEnvironment {
    /// From what the hub was started with, `started_with`, with `printer`, the daemon, printing what the login shell
    /// has. The profile's is read up front, on a task of its own, so the first workspace, or the first run of `gh`,
    /// doesn't wait for a slow login shell.
    pub(crate) fn new(started_with: Vec<(OsString, OsString)>, printer: PathBuf) -> Self {
        let started = Arc::new(Started::keep(started_with));
        let environment = Self { started, printer: printer.into(), whole: Arc::new(OnceCell::new()) };
        let early = environment.clone();
        tokio::spawn(async move {
            early.vars().await;
        });
        environment
    }

    /// `vars` and nothing else, with no profile read, for a test's hub.
    #[cfg(test)]
    pub(crate) fn only(vars: &[(&str, &str)]) -> Self {
        let vars = vars.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect();
        let printer = Path::new("").into();
        Self { started: Arc::default(), printer, whole: Arc::new(OnceCell::new_with(Some(vars))) }
    }

    /// This test's `PATH` alone, so what a test's hub starts finds programs as the test does, and has nothing else of
    /// the test's environment.
    #[cfg(test)]
    pub(crate) fn of_this_test() -> Self {
        Self::only(&[("PATH", &std::env::var("PATH").unwrap_or_default())])
    }

    /// Each variable, once the profile's have been read or couldn't be.
    pub(crate) async fn vars(&self) -> &Vars {
        self.whole.get_or_init(|| read(&self.started, &self.printer)).await
    }

    /// What a process the hub starts is given in place of what it would inherit: these variables, and `own` after.
    pub(crate) async fn given(&self, own: &[(String, String)]) -> Environment {
        Environment::whole(self.vars().await.iter().chain(own).map(|(name, value)| (name, value)))
    }
}

async fn read(started: &Started, printer: &Path) -> Vars {
    let shell = started.shell();
    let profile = profile::read(shell, &started.session, printer).await;
    if profile.is_some() {
        debug!(shell, "read the environment the profile gives");
    } else {
        warn!(
            shell,
            "couldn't read the environment the profile gives, so what the hub starts has only the session's variables \
             and the hub's own PATH"
        );
    }
    whole(started, profile)
}

/// The session's variables, then the profile's, or else the `PATH` the hub was started with, then the hub's own, each
/// in place of any of the same name before it.
pub(super) fn whole(started: &Started, profile: Option<Vars>) -> Vars {
    let mut whole = Vars::new();
    let path = started.path.iter().map(|path| ("PATH".to_string(), path.clone()));
    let profiles_or_path: Vars = profile.unwrap_or_else(|| path.collect());
    for (name, value) in started.session.iter().cloned().chain(profiles_or_path).chain(started.own.iter().cloned()) {
        whole.retain(|(before, _)| !same_name(before, &name));
        whole.push((name, value));
    }
    whole
}
