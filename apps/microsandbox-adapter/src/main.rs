//! `roundhouse-microsandbox`, the adapter of the microsandbox provider, which runs each workspace in a microVM of its
//! own on the hub's machine, through microsandbox. The hub runs it as `serve`, named in its `providers.json`. Each
//! sandbox's worker dials the hub with the ticket the hub gives at `start`, so the adapter is never in its stream, as
//! `docs/decisions/microsandbox-adapter.md` says. `docs/decisions/remote-workspace-vm.md` says why a workspace has a
//! VM of its own.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod guest;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod naming;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod provider;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sandbox;

#[derive(Parser)]
#[command(version)]
struct Args {
    /// The `msb` that runs the VMs, signed to use the hypervisor, with its `libkrunfw` beside it. Without it, the one
    /// microsandbox's own install put in its home.
    #[arg(long, global = true)]
    msb: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the provider protocol to a hub on stdin and stdout.
    Serve,
}

fn main() -> ExitCode {
    let args = Args::parse();
    // The hub keeps what an adapter writes to stderr in its log, so it's a line of JSON for each event, as the hub's are.
    let directives = std::env::var(protocol::paths::ENV_LOG).ok();
    let log = logging::Log::new(logging::Levels::new(directives.as_deref())).json_to(std::io::stderr).start();
    if let Err(e) = log {
        eprintln!("roundhouse-microsandbox: starting its log: {e}");
    }
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("roundhouse-microsandbox: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run(args: &Args) -> Result<ExitCode, String> {
    use std::sync::Arc;

    use providers::Provider;

    if let Some(msb) = &args.msb {
        microsandbox::config::set_sdk_msb_path(msb);
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("starting: {e}"))?;
    let Command::Serve = args.command;
    let make: providers::serve::MakeProvider =
        Box::new(move |hello| Ok(Arc::new(provider::Microsandbox::new(hello)) as Arc<dyn Provider>));
    let build = protocol::this_build!().version();
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    runtime
        .block_on(providers::serve::serve(make, build, stdin, stdout))
        .map_err(|e| format!("serving the hub: {e}"))?;
    Ok(ExitCode::SUCCESS)
}

/// microsandbox runs VMs only on a Mac with Apple silicon and on Linux with KVM.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn run(_args: &Args) -> Result<ExitCode, String> {
    Err("microsandbox runs only on macOS and Linux".to_owned())
}
