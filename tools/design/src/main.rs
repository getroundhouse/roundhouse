//! Writes `design/Index.bl`, a row for each canvas of the design with its status, from the badge at each one's top.
//! Brilliant lays it out when it's opened.

use std::fs;
use std::path::Path;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design");
    let written = design::index(&design::canvases(&design)?)?;
    let path = design.join(format!("{}.bl", design::INDEX));
    fs::write(&path, written).with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}
