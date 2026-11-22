//! What `iphone` is asked to do. The team, the app's identifier, the phone and the key are never kept in the repo, so
//! each comes from an option or, as CI and the README give them, the environment.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::connect::{IssuerId, Key, KeyId};
use crate::xcode::{App, BundleId, TeamId};

#[derive(Parser)]
#[command(about = "Builds the iPhone app, installs it on a phone, and uploads it to TestFlight")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Builds the app, signs it for development, and installs and opens it on a phone connected to this Mac. Xcode
    /// can register the phone and the identifier in the team's account, so run it only when that's meant
    Install {
        #[command(flatten)]
        app: AppArgs,
        /// The phone, as `xcrun devicectl list devices` shows it. Booted simulators are listed too, so it's never
        /// guessed
        #[arg(long, env = "DEVICE", hide_env_values = true)]
        device: String,
        /// A debug build, whose first layout overflows the 1 MB stack iOS gives the main thread
        #[arg(long)]
        debug: bool,
        /// Stop once it's signed
        #[arg(long)]
        no_install: bool,
    },
    /// Builds the Rust binary an upload packages, for any iPhone in release, and copies it to OUT
    Compile { out: PathBuf },
    /// Archives the app for any iPhone, has Xcode sign it for the App Store and upload it for the team's own testers,
    /// and waits for App Store Connect to process it, failing if it's rejected
    Upload {
        #[command(flatten)]
        app: AppArgs,
        #[command(flatten)]
        key: KeyArgs,
        /// A Rust binary `compile` made, rather than building one
        #[arg(long)]
        binary: Option<PathBuf>,
        /// What `TestFlight` tells testers to try, as `what-to-test` writes it
        #[arg(long)]
        notes: Option<PathBuf>,
    },
    /// Exits 0 if what the app is built from changed between COMMIT and HEAD, and 1 if not
    ChangedSince { commit: String },
    /// Prints what `TestFlight` should tell testers to try: HEAD, and the titles of the commits since COMMIT that
    /// changed what the app is built from
    WhatToTest {
        #[arg(long)]
        since: Option<String>,
    },
}

#[derive(Args)]
pub(crate) struct AppArgs {
    /// The Apple Developer team that signs it
    #[arg(long, env = "TEAM_ID", hide_env_values = true)]
    team_id: String,
    /// The app's identifier in that team
    #[arg(long, env = "BUNDLE_ID", hide_env_values = true)]
    bundle_id: String,
}

impl AppArgs {
    pub(crate) fn identify(self) -> anyhow::Result<App> {
        Ok(App { team: TeamId::new(self.team_id)?, bundle: BundleId::new(self.bundle_id)? })
    }
}

/// An App Store Connect key with the Admin role, which Xcode makes the distribution certificate and profile with.
#[derive(Args)]
pub(crate) struct KeyArgs {
    /// The key's .p8 file
    #[arg(long, env = "ASC_KEY_PATH", hide_env_values = true)]
    key_path: PathBuf,
    #[arg(long, env = "ASC_KEY_ID", hide_env_values = true)]
    key_id: String,
    #[arg(long, env = "ASC_ISSUER_ID", hide_env_values = true)]
    issuer_id: String,
}

impl KeyArgs {
    pub(crate) fn read(self) -> anyhow::Result<Key> {
        Key::read(self.key_path, KeyId::new(self.key_id), IssuerId::new(self.issuer_id))
    }
}
