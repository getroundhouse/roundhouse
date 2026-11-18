//! look's build script. It builds the tables the file icons are found in.

mod file_icons;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    file_icons::write(&dir.join("file-icons"), &out_dir)?;
    Ok(())
}
