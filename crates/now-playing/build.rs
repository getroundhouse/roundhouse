//! Compiles this crate, on a Mac, into the library `/usr/bin/perl` loads to pause what's playing while something is
//! dictated, and says where it is in `ROUNDHOUSE_NOW_PLAYING`. With `rustc` alone, since Cargo would build a library in
//! a folder whose name changes with the build, and tell the crate that depends on it nothing of where, so the app
//! couldn't find it. `tools/build.sh` puts it beside the binaries it builds.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

/// The library's file, which `tools/build.sh` looks for, and `src/mac.rs` takes from `ROUNDHOUSE_NOW_PLAYING`.
const LIBRARY: &str = "libroundhouse_now_playing.dylib";

/// Set only as the library is compiled, so it takes only the part perl runs.
const PERL_LIBRARY: &str = "perl_library";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rustc-check-cfg=cfg({PERL_LIBRARY})");
    if env::var("CARGO_CFG_TARGET_OS")? != "macos" {
        return Ok(());
    }
    println!("cargo::rerun-if-changed=src");
    let source = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("src/lib.rs");
    let library = PathBuf::from(env::var("OUT_DIR")?).join(LIBRARY);
    let status = Command::new(env::var("RUSTC")?)
        // The workspace's, which this crate is written in, as `[workspace.package]` in Cargo.toml says.
        .args(["--edition", "2024", "--crate-type", "cdylib", "--crate-name", "roundhouse_now_playing"])
        .args(["--target", &env::var("TARGET")?])
        .args(["--cfg", PERL_LIBRARY])
        // Small, since it's loaded on each dictation, and nothing it does is slow.
        .args(["-C", "opt-level=s", "-C", "panic=abort"])
        // Cargo compiles the same code into this crate's tests, where its lints are checked.
        .args(["--cap-lints", "allow"])
        .arg("-o")
        .arg(&library)
        .arg(&source)
        .status()?;
    if !status.success() {
        return Err(format!("rustc couldn't compile {}: {status}", source.display()).into());
    }
    println!("cargo::rustc-env=ROUNDHOUSE_NOW_PLAYING={}", library.display());
    Ok(())
}
