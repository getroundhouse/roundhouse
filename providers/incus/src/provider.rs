//! Each instance is a VM, copied from a base image's snapshot, which on ZFS is a clone and takes about a second. Its
//! worker runs as one user, so everything running as that user is the worker's or something it started, and ending
//! what a worker left behind is ending that user's processes.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{HubId, InstanceId, WorkerTarget};
use providers::{
    Capabilities, Feature, Instance, InstanceState, Provider, ProviderError, Size, WorkerBinary, WorkerCommand,
};
use tracing::info;

use crate::cli::{Incus, Listed, Status, failed};

/// Which hub made an instance, so each hub lists only its own.
const HUB_KEY: &str = "user.roundhouse.hub";
/// The hub's ID for the instance, which the VM's name is made from.
const INSTANCE_KEY: &str = "user.roundhouse.instance";
/// What the hub calls it, for someone reading `incus list`.
const NAME_KEY: &str = "user.roundhouse.name";
const BASE_IMAGE_KEY: &str = "user.roundhouse.base-image";
/// The snapshot of a base image that instances are copied from.
const BASE_SNAPSHOT: &str = "base";

/// A VM boots, and its agent answers, in about 9 s.
const BOOT_TIMEOUT: Duration = Duration::from_mins(3);
/// How often to ask whether a booting VM's agent answers. Each ask is an `incus exec`, which takes about a tenth of a
/// second, so this costs little and finds a VM ready soon after it is.
const BOOT_POLL: Duration = Duration::from_millis(100);
/// How long a worker gets to stop its agents, as on the local provider.
const LEFTOVER_GRACE: Duration = Duration::from_secs(10);
/// How long a VM gets to shut down before it becomes a base image, which has to be shut down cleanly, so its disks
/// are whole in the snapshot. Shorter than the call's own timeout, so `incus` says why it gave up.
const CLEAN_STOP_TIMEOUT: Duration = Duration::from_mins(2);
/// What the copy of a new instance is called until it's whole, so one cut short is never taken for the instance.
const MAKING_SUFFIX: &str = "-making";
/// Run in the VM as root, with the worker's user and how long it gets, in tenths of a second. Ends everything running
/// as the user, and exits 0 once nothing is, or 3 if something outlives SIGKILL, so any other status is `incus`'s own.
const END_USER_SCRIPT: &str = r#"user=$1 tenths=$2
if [ "$tenths" -gt 0 ]; then pkill -TERM -u "$user"; fi
i=0; while [ "$i" -lt "$tenths" ]; do pgrep -u "$user" >/dev/null || exit 0; sleep 0.1; i=$((i + 1)); done
pkill -KILL -u "$user"
i=0; while [ "$i" -lt 20 ]; do pgrep -u "$user" >/dev/null || exit 0; sleep 0.1; i=$((i + 1)); done
exit 3"#;
const SURVIVED_SIGKILL: i32 = 3;

pub(crate) struct Config {
    /// The base image new instances are copied from when the hub names none. *Not built:* the hub's own base
    /// images, one per repository.
    pub base_image: String,
    pub target: WorkerTarget,
    /// Who the worker runs as.
    pub user: u32,
    pub group: u32,
    pub home: String,
    pub workspaces_root: String,
    pub state_dir: String,
    /// The worker binary in the image. *Not built:* putting the hub's own in place as an instance starts.
    pub worker: String,
    /// What the worker's environment has besides `HOME` and `PATH`, such as the agent CLI to use.
    pub env: Vec<(String, String)>,
}

pub(crate) struct IncusProvider {
    incus: Incus,
    hub_id: HubId,
    config: Config,
}

/// The VM's name. Incus takes letters, digits and hyphens, and the hub's IDs are UUIDs.
fn vm_name(instance_id: &InstanceId) -> String {
    let id: String = instance_id
        .as_str()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    format!("rh-{id}")
}

fn state(listed: &Listed) -> InstanceState {
    match listed.status {
        Status::Running => InstanceState::Running,
        Status::Stopped => InstanceState::Stopped,
        Status::Starting => InstanceState::Starting,
        Status::Stopping => InstanceState::Stopping,
        Status::Other => InstanceState::Unknown,
    }
}

impl IncusProvider {
    pub(crate) fn new(incus: Incus, hub_id: HubId, config: Config) -> Self {
        Self { incus, hub_id, config }
    }

    /// The instance, if it's this hub's.
    async fn find(&self, instance_id: &InstanceId) -> Result<Option<Listed>, ProviderError> {
        let found = self.incus.find(&vm_name(instance_id)).await?;
        Ok(found.filter(|listed| listed.tag(HUB_KEY) == Some(self.hub_id.as_str())))
    }

    async fn wait_until_it_answers(&self, name: &str) -> Result<(), ProviderError> {
        let deadline = Instant::now() + BOOT_TIMEOUT;
        loop {
            if self.incus.exec(name, &["true"]).await.is_ok_and(|output| output.status.success()) {
                return Ok(());
            }
            if Instant::now() > deadline {
                let e = std::io::Error::new(std::io::ErrorKind::TimedOut, "its agent never answered");
                return Err(ProviderError::io(format!("starting {name}"), e));
            }
            tokio::time::sleep(BOOT_POLL).await;
        }
    }

    /// Ends everything running as the worker's user, giving it `grace` to finish first.
    async fn end_leftovers(&self, name: &str, grace: Duration) -> Result<(), ProviderError> {
        let (user, tenths) = (self.config.user.to_string(), (grace.as_millis() / 100).to_string());
        let what = format!("ending what runs as {user} in {name}");
        let ended = self.incus.exec(name, &["sh", "-c", END_USER_SCRIPT, "sh", &user, &tenths]).await?;
        match ended.status.code() {
            Some(0) => Ok(()),
            Some(SURVIVED_SIGKILL) => Err(ProviderError::io(what, std::io::Error::other("something outlived SIGKILL"))),
            _ => Err(failed(&what, &ended)),
        }
    }

    /// Waits for a VM that's stopping to have stopped, since Incus won't start it or take its snapshot until then.
    async fn wait_until_stopped(&self, name: &str) -> Result<(), ProviderError> {
        let deadline = Instant::now() + BOOT_TIMEOUT;
        while self.incus.find(name).await?.is_some_and(|listed| listed.status == Status::Stopping) {
            if Instant::now() > deadline {
                let e = std::io::Error::new(std::io::ErrorKind::TimedOut, "it never finished stopping");
                return Err(ProviderError::io(format!("waiting for {name} to stop"), e));
            }
            tokio::time::sleep(BOOT_POLL).await;
        }
        Ok(())
    }

    /// Stopping and killing leave the VM running, as on every provider, since the hub asks to stop whenever a worker's
    /// stream ends, such as for a worker that's being replaced, and sleeping the VM each time would cost a boot.
    /// Sleeping is a request of its own, which is yet to come.
    async fn end_worker(&self, instance_id: &InstanceId, grace: Duration) -> Result<(), ProviderError> {
        let Some(listed) = self.find(instance_id).await? else { return Ok(()) };
        if state(&listed) != InstanceState::Running {
            return Ok(());
        }
        self.end_leftovers(&listed.name, grace).await
    }
}

#[async_trait]
impl Provider for IncusProvider {
    fn name(&self) -> &'static str {
        "incus"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            targets: vec![self.config.target.clone()],
            features: vec![Feature::FilesystemSurvivesStop, Feature::BaseImages],
        }
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        _size: Option<Size>,
        display_name: &str,
    ) -> Result<(), ProviderError> {
        let name = vm_name(instance_id);
        // Only a whole copy has this name, so one that's there is done, if it's this hub's.
        if let Some(there) = self.incus.find(&name).await? {
            if there.tag(HUB_KEY) == Some(self.hub_id.as_str()) {
                return Ok(());
            }
            let e = ApiError::new(ErrorCode::Conflict, format!("{name} is another hub's, or was made by hand"));
            return Err(ProviderError::Adapter(e));
        }
        let making = format!("{name}{MAKING_SUFFIX}");
        // A copy cut short, such as by an adapter that was killed.
        if self.incus.find(&making).await?.is_some() {
            self.incus
                .run(&format!("deleting {making}"), &["delete", "--force", &self.incus.on_remote(&making)])
                .await?;
        }
        let base = from.map_or_else(|| self.config.base_image.clone(), vm_name);
        let source = self.incus.on_remote(&format!("{base}/{BASE_SNAPSHOT}"));
        let tags = [
            format!("{HUB_KEY}={}", self.hub_id),
            format!("{INSTANCE_KEY}={instance_id}"),
            format!("{NAME_KEY}={display_name}"),
            format!("{BASE_IMAGE_KEY}=false"),
        ];
        // A snapshot is copied alone, without the base image's other snapshots.
        let mut args = vec!["copy".to_string(), source, self.incus.on_remote(&making)];
        for tag in tags {
            args.extend(["--config".to_string(), tag]);
        }
        let started = Instant::now();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.incus.run(&format!("copying {base} to {making}"), &args).await?;
        let (from, to) = (self.incus.on_remote(&making), self.incus.on_remote(&name));
        self.incus.run(&format!("renaming {making} to {name}"), &["move", &from, &to]).await?;
        info!(name, base, took_ms = started.elapsed().as_millis(), "made an instance");
        Ok(())
    }

    /// Waits for its agent whether or not it started it, since Incus starts a VM by itself too, such as when its
    /// server boots.
    async fn start(&self, instance_id: &InstanceId, _workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        let listed = self.find(instance_id).await?.ok_or_else(|| ProviderError::NotFound(instance_id.clone()))?;
        let name = listed.name.clone();
        let started = Instant::now();
        let start = match listed.status {
            Status::Running | Status::Starting => false,
            Status::Stopping => {
                self.wait_until_stopped(&name).await?;
                true
            }
            Status::Stopped | Status::Other => true,
        };
        if start {
            self.incus.run(&format!("starting {name}"), &["start", &self.incus.on_remote(&name)]).await?;
        }
        self.wait_until_it_answers(&name).await?;
        info!(name, took_ms = started.elapsed().as_millis(), "its agent answers");
        Ok(())
    }

    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end_worker(instance_id, LEFTOVER_GRACE).await
    }

    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end_worker(instance_id, Duration::ZERO).await
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        let Some(listed) = self.find(instance_id).await? else { return Ok(()) };
        let target = self.incus.on_remote(&listed.name);
        self.incus.run(&format!("deleting {}", listed.name), &["delete", "--force", &target]).await?;
        info!(name = listed.name, "destroyed an instance");
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        let hub = self.hub_id.to_string();
        let ours = self.incus.list().await?.into_iter().filter(|listed| listed.tag(HUB_KEY) == Some(hub.as_str()));
        Ok(ours
            .filter_map(|listed| {
                Some(Instance {
                    instance_id: listed.tag(INSTANCE_KEY)?.into(),
                    state: state(&listed),
                    base_image: listed.tag(BASE_IMAGE_KEY) == Some("true"),
                })
            })
            .collect())
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        let listed = self.find(instance_id).await?.ok_or_else(|| ProviderError::NotFound(instance_id.clone()))?;
        Ok(worker_command(&self.incus, &self.config, &listed.name))
    }

    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        let listed = self.find(instance_id).await?.ok_or_else(|| ProviderError::NotFound(instance_id.clone()))?;
        let target = self.incus.on_remote(&listed.name);
        match listed.status {
            Status::Stopped => {}
            Status::Stopping => self.wait_until_stopped(&listed.name).await?,
            Status::Running | Status::Starting | Status::Other => {
                let timeout = CLEAN_STOP_TIMEOUT.as_secs().to_string();
                self.incus.run(&format!("stopping {}", listed.name), &["stop", &target, "--timeout", &timeout]).await?;
            }
        }
        let snapshots = self.incus.run("listing snapshots", &["snapshot", "list", &target, "--format", "csv"]).await?;
        if !snapshots.lines().any(|line| line.split(',').next() == Some(BASE_SNAPSHOT)) {
            self.incus.run("taking a snapshot", &["snapshot", "create", &target, BASE_SNAPSHOT]).await?;
        }
        let mark = format!("{BASE_IMAGE_KEY}=true");
        self.incus.run("marking a base image", &["config", "set", &target, &mark]).await?;
        Ok(())
    }
}

/// `incus exec` as the worker's user, with its stdin and stdout as the worker's stream, and no terminal, which
/// would mangle them.
pub(crate) fn worker_command(incus: &Incus, config: &Config, name: &str) -> WorkerCommand {
    let path = format!("{}/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", config.home);
    let mut args = vec!["--project".to_string(), incus.project.clone(), "exec".to_string(), incus.on_remote(name)];
    let user_and_place = [
        "-T",
        "--user",
        &config.user.to_string(),
        "--group",
        &config.group.to_string(),
        "--cwd",
        &config.home,
        "--env",
        &format!("HOME={}", config.home),
        "--env",
        &format!("PATH={path}"),
    ]
    .map(str::to_string);
    args.extend(user_and_place);
    for (key, value) in &config.env {
        args.extend(["--env".to_string(), format!("{key}={value}")]);
    }
    args.extend(
        ["--", &config.worker, "worker", "--stdio", "--workspaces-root", &config.workspaces_root, "--state-dir"]
            .map(str::to_string),
    );
    args.push(config.state_dir.clone());
    WorkerCommand { program: incus.program.display().to_string(), args, env: vec![] }
}

#[cfg(test)]
mod tests;
