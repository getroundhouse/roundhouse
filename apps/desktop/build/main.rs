//! The desktop app's build script. It builds the tables the file icons are found in, and puts the app's own icon in
//! the program when it's built for Windows.

mod app_icon;
mod file_icons;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

/// The app's icon, which `tools/package/ico.py` makes from its drawings, for the installer too.
const APP_ICON: &str = "icons/AppIcon.ico";

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    file_icons::write(&dir.join("file-icons"), &out_dir)?;
    // Only for MSVC's ABI, since MSVC's linker and LLVM's take a `.res` as it is. A build for `windows-gnu` links with
    // GNU's ld, which isn't one of them, so it goes without the icon rather than risk a program that doesn't link.
    let windows = env::var("CARGO_CFG_TARGET_OS")? == "windows";
    if windows && env::var("CARGO_CFG_TARGET_ENV")? == "msvc" {
        app_icon::embed(&dir.join(APP_ICON), &out_dir)?;
    }
    Ok(())
}
