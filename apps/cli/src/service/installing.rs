//! `roundhouse hub install` and `roundhouse hub uninstall`: what each checks, does and says.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use client_core::HubAdmin;
use protocol::client::HubStatus;
use protocol::paths::{self, Paths};

use super::definition::Service;
use super::manager;
use super::service_user::{self, ServiceUser};

/// How long the service's hub has to start listening once it's registered, which a hub that migrates a large database
/// as it starts can take a while to.
const STARTING: Duration = Duration::from_secs(30);

/// Places this build in `user`'s data directory, as that user, registers the service, which starts its hub, and waits
/// for the hub to answer.
pub(crate) fn install(user: &str) -> anyhow::Result<()> {
    let service_user = ServiceUser::named(user)?;
    anyhow::ensure!(service_user::is_administrator(), "registering a service takes an administrator. Run it with sudo");
    let build = crate::BUILD.stamped().context("only a build stamped with its version can be installed")?;
    let this = std::env::current_exe().context("finding this program")?;
    let daemon = this.with_file_name(paths::daemon_file());
    check_daemon(&daemon)?;
    let paths = paths_of(&service_user)?;
    place_as(&service_user, &this)?;
    manager::register(&service_of(&service_user, &paths))?;
    let status = answered(&paths.socket).context("waiting for the hub to start")?;
    println!("installed build {} as a service, running as {user}, which started its hub", build.version());
    println!("pid {}  version {}", status.pid, status.hub_version);
    println!("its log is {}", paths.log.display());
    let cli = paths.service_current().join(paths::cli_file());
    println!("pair a device with it: sudo -u {user} -H '{}' phone pair", cli.display());
    Ok(())
}

/// Removes the service, which stops its hub, and leaves the hub's data and builds where they are.
pub(crate) fn uninstall() -> anyhow::Result<()> {
    anyhow::ensure!(service_user::is_administrator(), "removing a service takes an administrator. Run it with sudo");
    manager::unregister()?;
    println!("removed the service and stopped its hub. Its data and builds are still in its user's data directory");
    Ok(())
}

/// Refuses a daemon beside this program that's missing, or another build, since the service would run it.
fn check_daemon(daemon: &Path) -> anyhow::Result<()> {
    let expected = paths::daemon_version_line(crate::BUILD.version());
    process::check_version(daemon, &expected)
        .with_context(|| format!("checking {}, which the service would run", daemon.display()))
}

/// Where `service_user`'s hub keeps its files, as their own commands find it, from their home directory alone.
fn paths_of(service_user: &ServiceUser) -> anyhow::Result<Paths> {
    let home = service_user.home.to_string_lossy().into_owned();
    paths::resolve(|name| (name == paths::HOME_VAR).then(|| home.clone())).map_err(anyhow::Error::msg)
}

/// Runs `hub place` from `this`, as `service_user`, with nothing of this process's environment but `PATH`, so what it
/// makes is theirs and where their own commands look.
fn place_as(service_user: &ServiceUser, this: &Path) -> anyhow::Result<()> {
    let mut command = Command::new(this);
    command.args(["hub", "place"]).env_clear().env(paths::HOME_VAR, &service_user.home).current_dir(&service_user.home);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    service_user.runs(&mut command);
    let output = command.stdin(Stdio::null()).output().context("placing this build as the hub's user")?;
    anyhow::ensure!(
        output.status.success(),
        "placing this build as {} failed: {}",
        service_user.name,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// The service for `service_user`'s hub. Its environment says where the hub's data and workspaces are, as a client
/// says to a hub it starts, and gives it the user's home, name and shell, and this process's `PATH`, which the service
/// manager would otherwise leave out or set its own way.
fn service_of(service_user: &ServiceUser, paths: &Paths) -> Service {
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let mut environment = vec![
        (paths::HOME_VAR, text(&service_user.home)),
        ("USER", service_user.name.clone()),
        ("LOGNAME", service_user.name.clone()),
        (paths::ENV_HOME, text(&paths.data_dir)),
        (paths::ENV_WORKSPACES, text(&paths.workspaces_root)),
    ];
    environment.extend(service_user.shell.clone().map(|shell| ("SHELL", shell)));
    environment.extend(std::env::var("PATH").ok().map(|path| ("PATH", path)));
    Service {
        user: service_user.name.clone(),
        program: paths.service_current().join(paths::daemon_file()),
        working_dir: paths.data_dir.clone(),
        log: paths.log.clone(),
        environment,
    }
}

/// The status of the hub at `socket`, once it answers, which it does once the service has started it.
#[expect(clippy::disallowed_methods, reason = "the pace of looking again while the service's hub begins to listen")]
fn answered(socket: &Path) -> anyhow::Result<HubStatus> {
    let deadline = Instant::now() + STARTING;
    loop {
        match HubAdmin::connect(socket).and_then(|hub| hub.status()) {
            Ok(status) => return Ok(status),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.into()),
        }
    }
}
