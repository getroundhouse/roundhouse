//! `guide OUT` builds the guide into OUT, which must not exist: `site/docs` in the site as it's published.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use guide::Source;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (Some(out), None) = (args.next(), args.next()) else { bail!("usage: guide OUT") };
    let out = PathBuf::from(out);
    // This is run from a checkout, by `cargo run -p guide`, so the checkout it was built in is the one it reads.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = repo.canonicalize().with_context(|| format!("couldn't find the repository at {}", repo.display()))?;

    let built = guide::build(&Source::in_repo(&repo), &out)?;

    println!("{} pages and {} pictures in {}", built.pages, built.pictures, out.display());
    Ok(())
}
