//! Running the programs that build, sign and upload the app: Cargo, Git, `XcodeGen`, `xcodebuild` and `devicectl`.

use std::process::Command;

use anyhow::{Context as _, bail};

/// Runs `command` with this process's output, and fails unless it succeeds.
pub(crate) fn run(command: &mut Command) -> anyhow::Result<()> {
    let status = command.status().with_context(|| format!("couldn't run {}", name(command)))?;
    if !status.success() {
        bail!("{} failed, with {status}", name(command));
    }
    Ok(())
}

/// What `command` prints, once it has succeeded. What it says of errors goes to this process's.
pub(crate) fn output(command: &mut Command) -> anyhow::Result<String> {
    let output = command
        .stderr(std::process::Stdio::inherit())
        .output()
        .with_context(|| format!("couldn't run {}", name(command)))?;
    if !output.status.success() {
        bail!("{} failed, with {}", name(command), output.status);
    }
    String::from_utf8(output.stdout).with_context(|| format!("{} printed what isn't UTF-8", name(command)))
}

/// The program and its first argument, such as `git diff`, which says which step it was.
fn name(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    match command.get_args().next() {
        Some(first) => format!("`{program} {}`", first.to_string_lossy()),
        None => format!("`{program}`"),
    }
}
