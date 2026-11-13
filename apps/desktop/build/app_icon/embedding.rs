use super::res;
use anyhow::Context as _;
use std::fs;
use std::path::Path;

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
