//! Puts the app's icon in `roundhouse-desktop.exe`, as icon 1. GPUI's Windows backend loads icon 1 from its own program
//! once, as its platform starts, and registers it on the one window class its windows are made from. Windows shows a
//! class's icon on each of its windows and on their taskbar button.
//!
//! The icon is the `.ico` that `tools/package/ico.py` makes from the app's drawings, which the installer has too, so
//! this knows nothing of the drawings. It turns the `.ico` into a compiled resource file, a `.res`, as `rc.exe` would,
//! and has the linker put that in the program. MSVC's linker and LLVM's both take a `.res` as it is, so building needs
//! no resource compiler, on Windows or from a Mac.

mod res;

use std::fs;
use std::path::Path;

use anyhow::Context as _;

/// Has the linker put the icon in `ico` in `roundhouse-desktop.exe`, from a `.res` it writes to `out_dir`.
pub(crate) fn embed(ico: &Path, out_dir: &Path) -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed={}", ico.display());
    let icon = fs::read(ico).with_context(|| format!("couldn't read {}", ico.display()))?;
    let compiled = res::from_ico(&icon).with_context(|| format!("couldn't make a .res from {}", ico.display()))?;
    let path = out_dir.join("app-icon.res");
    fs::write(&path, compiled).with_context(|| format!("couldn't write {}", path.display()))?;
    println!("cargo::rustc-link-arg-bin=roundhouse-desktop={}", path.display());
    Ok(())
}
