//! Running the `incus` CLI against one remote and project, and reading what it says. The adapter keeps to the CLI
//! rather than Incus's REST API, so it uses the remotes and certificates `incus remote add` set up, and the hub holds
//! no secrets for it.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use providers::ProviderError;
use serde::Deserialize;
use tokio::process::Command;

/// Far longer than any `incus` call should take, the slowest being a copy that ZFS doesn't make a clone, and shorter
/// than the hub's deadline for the request, so the adapter answers first.
const CALL_TIMEOUT: Duration = Duration::from_mins(5);
/// How many times to ask again about an instance Incus lists without its config, which it does while it's stopping.
const CONFIG_TRIES: u32 = 40;
const CONFIG_WAIT: Duration = Duration::from_millis(250);
/// How Incus starts refusing what a project's limits don't leave room for.
const LIMIT_REACHED: &str = "Reached maximum aggregate value";

pub(crate) struct Incus {
    pub(crate) program: PathBuf,
    pub(crate) remote: String,
    pub(crate) project: String,
}

/// An instance, as `incus list --format json` has it.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) name: String,
    pub(crate) status: Status,
    /// `None` while it's stopping, when Incus lists it without, so what it says can't be read until Incus is asked
    /// again.
    #[serde(default)]
    pub(crate) config: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Running,
    Stopped,
    Starting,
    Stopping,
    /// One this doesn't know, such as `Frozen` or `Error`.
    #[serde(other)]
    Other,
}

impl Listed {
    /// What its config says `key` is.
    pub(crate) fn tag(&self, key: &str) -> Option<&str> {
        self.config.as_ref()?.get(key).map(String::as_str)
    }
}

impl Incus {
    /// `name` on the remote, as the CLI spells it.
    pub(crate) fn on_remote(&self, name: &str) -> String {
        format!("{}:{name}", self.remote)
    }

    /// Runs `incus` with `args` in the project, and what it printed if it succeeded.
    pub(crate) async fn run(&self, what: &str, args: &[&str]) -> Result<String, ProviderError> {
        let output = self.output(what, args).await?;
        if !output.status.success() {
            return Err(failed(what, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs `incus` with `args` in the project, whatever its exit status.
    pub(crate) async fn output(&self, what: &str, args: &[&str]) -> Result<Output, ProviderError> {
        // Before the command's own, since what follows `--` in `exec` is the instance's.
        let running =
            Command::new(&self.program).args(["--project", &self.project]).args(args).kill_on_drop(true).output();
        let ran = tokio::time::timeout(CALL_TIMEOUT, running).await.map_err(|_| {
            ProviderError::io(
                what,
                io::Error::new(io::ErrorKind::TimedOut, format!("incus took over {CALL_TIMEOUT:?}")),
            )
        })?;
        ran.map_err(|e| ProviderError::io(format!("{what}: running {}", self.program.display()), e))
    }

    /// Every instance in the project, once Incus lists each with its config.
    pub(crate) async fn list(&self) -> Result<Vec<Listed>, ProviderError> {
        let remote = format!("{}:", self.remote);
        for _ in 0..CONFIG_TRIES {
            let json = self.run("listing instances", &["list", &remote, "--format", "json"]).await?;
            let listed = parse_list(&json)
                .map_err(|e| ProviderError::io("reading the list of instances", io::Error::other(e)))?;
            if listed.iter().all(|instance| instance.config.is_some()) {
                return Ok(listed);
            }
            tokio::time::sleep(CONFIG_WAIT).await;
        }
        let e = io::Error::new(io::ErrorKind::TimedOut, "incus kept listing an instance without its config");
        Err(ProviderError::io("listing instances", e))
    }

    pub(crate) async fn find(&self, name: &str) -> Result<Option<Listed>, ProviderError> {
        Ok(self.list().await?.into_iter().find(|instance| instance.name == name))
    }

    /// Runs `command` in the instance as root, whatever its exit status.
    pub(crate) async fn exec(&self, name: &str, command: &[&str]) -> Result<Output, ProviderError> {
        let target = self.on_remote(name);
        let mut args = vec!["exec", &target, "--"];
        args.extend_from_slice(command);
        self.output(&format!("running {} in {name}", command.join(" ")), &args).await
    }
}

/// What `incus` said when it failed. A project's limit is a provider with no room, as the protocol has it.
pub(crate) fn failed(what: &str, output: &Output) -> ProviderError {
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if said.contains(LIMIT_REACHED) {
        return ProviderError::Adapter(ApiError::new(ErrorCode::Unavailable, format!("{what}: {said}")));
    }
    ProviderError::io(what, io::Error::other(said))
}

pub(crate) fn parse_list(json: &str) -> serde_json::Result<Vec<Listed>> {
    serde_json::from_str(json)
}
