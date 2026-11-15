//! The versions in `Cargo.toml` that move with the copy: gpui-kit's, and the GPUI the app draws with off the main
//! thread, which has to be the kit's.

use std::fs;
use std::path::Path;

use toml::{Table, Value};

use crate::copy::COPY;
use crate::error::{At, Error};

/// The versions `Cargo.toml` was moved to, as it writes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    /// gpui-kit's, such as `=0.9.0`.
    pub kit: String,
    /// gpui-pre-apple's, such as `=0.3.11`, which is the GPUI the kit's workspace pins.
    pub gpui: String,
}

/// Moves `gpui-kit` in `Cargo.toml` to the version of the copy's, and `gpui-pre-apple` to the GPUI the copy pins.
/// Returns them if they moved.
pub(crate) fn pin_versions(root: &Path) -> Result<Option<Versions>, Error> {
    let copy = root.join(COPY);
    let workspace = read(&copy.join("Cargo.toml"))?;
    let kit = read(&copy.join("crates/kit/Cargo.toml"))?;
    let kit = (kit.get("package").and_then(|package| package.get("version")).and_then(Value::as_str))
        .ok_or(Error::NoVersion("gpui-kit's version, in crates/kit/Cargo.toml"))?;
    let gpui = (workspace.get("workspace").and_then(|workspace| workspace.get("dependencies")))
        .and_then(|dependencies| dependencies.get("gpui"))
        .and_then(|gpui| gpui.get("version"))
        .and_then(Value::as_str)
        .ok_or(Error::NoVersion("the version of GPUI the workspace depends on, in Cargo.toml"))?;
    let versions = Versions { kit: format!("={kit}"), gpui: gpui.to_string() };

    let manifest = root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest).at(&manifest)?;
    let moved = moved(&moved(&text, "gpui-kit", &versions.kit)?, "gpui-pre-apple", &versions.gpui)?;
    if moved == text {
        return Ok(None);
    }
    fs::write(&manifest, moved).at(&manifest)?;
    Ok(Some(versions))
}

fn read(path: &Path) -> Result<Table, Error> {
    let text = fs::read_to_string(path).at(path)?;
    text.parse().map_err(|source| Error::Toml { path: path.to_path_buf(), source })
}

/// `manifest` with the version on the one line that gives `name` one moved to `version`.
fn moved(manifest: &str, name: &'static str, version: &str) -> Result<String, Error> {
    const SAYS: &str = "version = \"";
    let start = format!("{name} = {{ ");
    let mut lines = Vec::new();
    let mut at = 0;
    for line in manifest.split_inclusive('\n') {
        if line.starts_with(&start)
            && let Some(says) = line.find(SAYS)
        {
            lines.push(at + says + SAYS.len());
        }
        at += line.len();
    }
    let [value] = lines[..] else { return Err(Error::NoVersionLine(name)) };
    let end = manifest[value..].find('"').ok_or(Error::NoVersionLine(name))?;
    Ok(format!("{}{version}{}", &manifest[..value], &manifest[value + end..]))
}
