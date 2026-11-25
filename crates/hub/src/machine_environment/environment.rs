use std::path::Path;
use std::sync::Arc;

use process::Environment;
use tracing::{debug, warn};

use super::login_shell::{LoginShell, Unread, Vars, deadline};

/// Whether this platform has a login shell whose profile gives the user's environment. Windows has none, and starts
/// every process with the user's and the system's variables already, and a shell that could be asked there, such as
/// Git's, would give them in a form of its own. So there, what the hub starts keeps what it inherits.
const ASKS_A_LOGIN_SHELL: bool = cfg!(unix);

/// What everything the hub starts on its machine is given over the environment it inherits from the hub: a local
/// worker, and so its agents, scripts, git and terminals, a provider's command and adapter, a preflight, `gh` and
/// `ssh`. It's what the user's login shell has once it has read their profile, started in the hub's own environment,
/// so a variable the profile exports reaches them however the hub was started, from the Dock or as a service at boot,
/// and one the profile doesn't set keeps the hub's value. The daemon removes a parent Claude session's variables as it
/// starts, whatever it runs as, the printer the login shell runs included, so a profile can't bring them back. Read
/// once, as the hub starts, and clones share it.
/// Not `Debug`, since its values can be secrets, such as a `GH_TOKEN` the profile exports, which the hub's log mustn't
/// show.
#[derive(Clone)]
pub(crate) struct MachineEnvironment {
    vars: Arc<[(String, String)]>,
}

impl MachineEnvironment {
    /// Asks `login`, which runs `printer`, the daemon, to print what it has. When it can't say, or there's no login
    /// shell to ask, it's only `PATH`: the login shell's, if it could say that much, then the hub's own, `hubs_path`,
    /// as before the hub read the profile, and the hub's log says why.
    pub(crate) async fn read(login: Option<&LoginShell<'_>>, printer: &Path, hubs_path: Option<&str>) -> Self {
        let Some(login) = login.filter(|_| ASKS_A_LOGIN_SHELL) else {
            debug!(
                name: "environment.no_login_shell",
                "There's no login shell to ask, so what the hub starts has what the hub was started with"
            );
            return Self::new(merged(Vars::new(), hubs_path));
        };
        let until = deadline();
        let vars = match login.environment(printer, until).await {
            Ok(vars) => {
                debug!(name: "environment.read", shell = login.shell, "Read the environment the profile gives");
                vars
            }
            Err(why) => {
                let path = match why {
                    // A shell that ran and couldn't run the printer, as one whose language isn't the POSIX shell's
                    // may not, can still say its `PATH`, as it did before the hub read more. One that couldn't
                    // start, or took too long, would do the same again. Both share one time limit, so the hub waits no
                    // longer for two.
                    Unread::Unsaid => login.path(until).await.ok(),
                    Unread::Unstarted(_) | Unread::TimedOut => None,
                };
                warn!(
                    name: "environment.unread",
                    shell = login.shell,
                    has_its_path = path.is_some(),
                    why = %why,
                    "Couldn't read the environment the profile gives, so what the hub starts has the hub's own, and \
                     the login shell's PATH if it said it"
                );
                path.map(|path| ("PATH".to_string(), path)).into_iter().collect()
            }
        };
        Self::new(merged(vars, hubs_path))
    }

    fn new(vars: Vars) -> Self {
        Self { vars: vars.into() }
    }

    /// `vars` and nothing else, as a test's hub is given, with nothing read.
    #[cfg(test)]
    pub(crate) fn only(vars: &[(&str, &str)]) -> Self {
        Self::new(vars.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect())
    }

    /// What it gives `name`.
    #[cfg(test)]
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.vars.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str())
    }

    /// What a process the hub starts is given over what it inherits.
    pub(crate) fn given(&self) -> Environment {
        self.vars.iter().fold(Environment::NONE, |given, (name, value)| given.with(name, value))
    }

    /// What a process whose command sets `own` is given: these variables, and `own` over them.
    pub(crate) fn with(&self, own: &[(String, String)]) -> Environment {
        own.iter().fold(self.given(), |given, (name, value)| given.with(name, value))
    }
}

/// `vars`, with `PATH` the login shell's entries, then each of the hub's own, `hubs_path`, it doesn't have. A profile can
/// set `PATH` outright, as Debian's `/etc/profile` does, and the hub's can hold tools added for this launch.
pub(super) fn merged(mut vars: Vars, hubs_path: Option<&str>) -> Vars {
    let login = vars.iter().position(|(name, _)| name == "PATH").map(|at| vars.remove(at).1);
    vars.extend(merge_paths(login.as_deref(), hubs_path).map(|path| ("PATH".to_string(), path)));
    vars
}

/// `login`'s entries first, then each of `own`'s it doesn't have.
fn merge_paths(login: Option<&str>, own: Option<&str>) -> Option<String> {
    let mut seen = Vec::new();
    for entry in login.into_iter().chain(own).flat_map(std::env::split_paths) {
        if !entry.as_os_str().is_empty() && !seen.contains(&entry) {
            seen.push(entry);
        }
    }
    if seen.is_empty() {
        return None;
    }
    // Both came in as text and were split where a `PATH` is, so the entries join again, and into text.
    std::env::join_paths(seen).ok()?.into_string().ok()
}
