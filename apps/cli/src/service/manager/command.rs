//! Running a service manager's own command line.

use std::process::Command;

use anyhow::Context;

/// Runs `program` with `args`, and fails with what it wrote to stderr if it fails.
pub(super) fn run(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(program).args(args).output().with_context(|| format!("running {program}"))?;
    anyhow::ensure!(
        output.status.success(),
        "{program} {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}
