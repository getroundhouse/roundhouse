//! `roundhouse-microsandbox`, the adapter of the microsandbox provider, which runs each workspace in a microVM of its
//! own on the hub's machine, through microsandbox. The hub runs it as `serve`, named in its `providers.json`, and
//! runs it as `connect` to reach a sandbox's worker, as `serve` says to. `docs/decisions/remote-workspace-vm.md`
//! says why a workspace has a VM of its own.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod connect;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod guest;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod naming;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod provider;

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
    Serve {
        /// The OCI image each sandbox is made from, which the setup script sets up.
        #[arg(long, default_value = "ubuntu:24.04")]
        image: String,
        /// A script to set each sandbox up with in place of the adapter's own, run as root once it has booted.
        #[arg(long)]
        setup: Option<PathBuf>,
    },
    /// Stream stdin and stdout to and from the worker in a sandbox, as the hub's worker stream.
    Connect { sandbox: String },
}

fn main() -> ExitCode {
    let args = Args::parse();
    // The hub keeps what an adapter and the command that reaches a worker write to stderr in its log.
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false).init();
    match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("roundhouse-microsandbox: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run(args: Args) -> Result<ExitCode, String> {
    use std::sync::Arc;

    use providers::Provider;

    if let Some(msb) = &args.msb {
        microsandbox::config::set_sdk_msb_path(msb);
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("starting: {e}"))?;
    match args.command {
        Command::Serve { image, setup } => {
            let setup = match setup {
                Some(path) => std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?,
                None => guest::SETUP.to_owned(),
            };
            let settings = provider::Settings { image, setup, msb: args.msb };
            let make: providers::serve::MakeProvider =
                Box::new(
                    move |hub_id| Ok(Arc::new(provider::Microsandbox::new(hub_id, settings)) as Arc<dyn Provider>),
                );
            let build = protocol::this_build!().version();
            let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
            runtime
                .block_on(providers::serve::serve(make, build, stdin, stdout))
                .map_err(|e| format!("serving the hub: {e}"))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Connect { sandbox } => runtime.block_on(connect::connect(&sandbox)),
    }
}

/// microsandbox runs VMs only on a Mac with Apple silicon and on Linux with KVM.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn run(_args: Args) -> Result<ExitCode, String> {
    Err("microsandbox runs only on macOS and Linux".to_owned())
}
