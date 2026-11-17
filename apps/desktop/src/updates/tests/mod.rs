//! The updater against a feed served on this machine, read as Velopack reads it: what a look downloads or refuses,
//! in `looks`, and what the app shows and installs, in `app`.

use std::fmt::Write as _;
use std::path::Path;

use gpui_kit::{IntoElement, Render, TestAppContext, Window};
use protocol::versions::Versions;
use ring::digest::{SHA1_FOR_LEGACY_USE_ONLY, SHA256, digest};
use ring::signature::Ed25519KeyPair;
use velopack::locator::VelopackLocatorConfig;

use super::*;
use crate::updates::signing::{key_pair, public};
use fetching::serving::{Reply, Server, serve};

mod app;
mod looks;

use looks::{LOOK_TIMEOUT, feeds_read, installed, manager_keeping, package_name, publish, publish_speaking};
