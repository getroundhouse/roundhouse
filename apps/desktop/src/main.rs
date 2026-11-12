// A release build opens no console window on Windows, which would otherwise give every program one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use desktop::checkout::{Checkout, take_note};

fn main() {
    desktop::appimage::leave_behind();
    desktop::updates::run_hooks();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    desktop::run(Checkout::of_this_build(take_note(&mut args)));
}
