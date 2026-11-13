//! Draws the spec as a page to open in a browser, in `.context/spec/`: each area's screens beside the rules they go
//! with, and which the owner has confirmed. Beside it, `index.json` holds what the page was drawn from.

mod index;
mod page;

use std::path::Path;

use anyhow::Result;

fn main() -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spec = repo.join("docs/spec");
    let index = index::read(&spec)?;
    let page = page::write(&index, &spec, &repo.join(".context/spec"))?;
    println!("{}", page.display());
    Ok(())
}
