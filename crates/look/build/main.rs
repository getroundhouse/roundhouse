//! look's build script. It builds the tables the file icons are found in.

mod file_icons;

use std::env;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    // Read as the script runs, not as it's built, so a build directory copied from another checkout finds this
    // one's files rather than that one's.
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    file_icons::write(&dir.join("file-icons"), &out_dir)?;
    Ok(())
}
