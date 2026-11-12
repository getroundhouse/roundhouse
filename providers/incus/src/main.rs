//! The Incus provider's adapter. A hub runs it, names it in its providers, and speaks the provider protocol to it on
//! its stdin and stdout. Each workspace runs in a VM of its own on an Incus server, copied from a base image, and
//! its worker's stream is `incus exec`, which the hub runs itself.
//!
//! A proof of concept: the base image comes from the command line, with Roundhouse's worker and an agent CLI already
//! in it, until the hub builds base images and places its own worker.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use protocol::model::WorkerTarget;
use providers::Provider;
use tracing_subscriber::EnvFilter;

mod cli;
mod provider;

use cli::Incus;
use provider::{Config, IncusProvider};

#[derive(Parser)]
#[command(name = "roundhouse-incus", version = protocol::BUILD_VERSION, about = "Runs Roundhouse workspaces in Incus VMs")]
struct Args {
    /// The `incus` CLI. Found on `PATH` if not given.
    #[arg(long)]
    incus: Option<PathBuf>,
    /// The remote, as `incus remote list` names it.
    #[arg(long, default_value = "local")]
    remote: String,
    #[arg(long, default_value = "default")]
    project: String,
    /// The VM, with a snapshot named `base`, that new instances are copied from.
    #[arg(long)]
    base_image: String,
    /// The Rust target the VMs' workers are built for.
    #[arg(long, default_value = "x86_64-unknown-linux-gnu")]
    target: String,
    /// The user the worker runs as, by ID. Everything running as this user is ended when a worker is, so it has to
    /// be one that runs nothing else.
    #[arg(long)]
    user: u32,
    /// The worker's group, by ID. The user's ID if not given.
    #[arg(long)]
    group: Option<u32>,
    #[arg(long, default_value = "/home/roundhouse")]
    home: String,
    #[arg(long, default_value = "/home/roundhouse/workspaces")]
    workspaces_root: String,
    #[arg(long, default_value = "/var/lib/roundhouse/instance")]
    state_dir: String,
    /// The worker binary in the image.
    #[arg(long, default_value = "/usr/local/bin/roundhoused")]
    worker: String,
    /// Something more for the worker's environment, as NAME=VALUE, such as the agent CLI to use.
    #[arg(long = "env", value_parser = name_and_value)]
    env: Vec<(String, String)>,
}

fn name_and_value(text: &str) -> Result<(String, String), String> {
    let (name, value) = text.split_once('=').ok_or("expected NAME=VALUE")?;
    Ok((name.to_string(), value.to_string()))
}

fn on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|candidate| candidate.is_file())
}

fn main() -> anyhow::Result<()> {
    // Logs go to stderr, because stdout is the protocol stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env(protocol::paths::ENV_LOG).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let args = Args::parse();
    // An absolute path, since the hub runs the `incus exec` that `connect` answers with, and its `PATH` may differ.
    let program = match args.incus {
        Some(program) => program,
        None => on_path("incus").context("finding the incus CLI on PATH")?,
    };
    let incus = Incus { program, remote: args.remote, project: args.project };
    let config = Config {
        base_image: args.base_image,
        target: WorkerTarget::from(args.target),
        user: args.user,
        group: args.group.unwrap_or(args.user),
        home: args.home,
        workspaces_root: args.workspaces_root,
        state_dir: args.state_dir,
        worker: args.worker,
        env: args.env,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(providers::serve::serve(
        Box::new(move |hub_id| {
            let provider: Arc<dyn Provider> = Arc::new(IncusProvider::new(incus, hub_id.clone(), config));
            Ok(provider)
        }),
        protocol::BUILD_VERSION,
        tokio::io::stdin(),
        tokio::io::stdout(),
    ))?;
    Ok(())
}
