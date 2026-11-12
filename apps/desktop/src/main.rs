// A release build opens no console window on Windows, which would otherwise give every program one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use desktop::checkout::{Checkout, built_from, take_note};

/// Which build this is, from the stamp it was compiled with.
const BUILD: protocol::Build = protocol::this_build!();
const BUILT_FROM: Option<&str> = built_from(BUILD, env!("CARGO_MANIFEST_DIR"));

fn main() {
    desktop::appimage::leave_behind();
    desktop::updates::run_hooks();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    desktop::run(BUILD, Checkout::of_this_build(BUILT_FROM, take_note(&mut args)));
}
