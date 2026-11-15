//! One command's words, read for the one program it runs.

use protocol::capability::calls::call::CommandPart;

use super::programs;
use super::words::{Words, pipelines};

/// What `command`, as its agent ran it, does: what one program does, or [`CommandPart::Other`].
pub(crate) fn of(command: &str) -> Vec<CommandPart> {
    vec![one_program(command).as_ref().and_then(programs::part).unwrap_or(CommandPart::Other)]
}

/// The words of the one program `command` runs, inside the shell it's run with, where it runs one: alone, or piped
/// into nothing but `sed -n` printing a range of lines, which only cuts what it prints short.
fn one_program(command: &str) -> Option<Words> {
    let pipelines = pipelines(&script(command)?)?;
    let [pipeline] = pipelines.as_slice() else { return None };
    let (program, rest) = pipeline.split_first()?;
    rest.iter().all(programs::prints_lines).then(|| program.clone())
}

/// The script `command` runs, inside the `sh`, `bash` or `zsh` it's run with by `-c` or `-lc`, as Codex runs every
/// command, or `command` itself.
fn script(command: &str) -> Option<String> {
    let wrapped = pipelines(command)?;
    if let [pipeline] = wrapped.as_slice()
        && let [words] = pipeline.as_slice()
        && let [shell, flag, script] = words.as_slice()
        && is_shell(shell)
        && (flag == "-c" || flag == "-lc")
    {
        return Some(script.clone());
    }
    Some(command.to_owned())
}

fn is_shell(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, "sh" | "bash" | "zsh")
}
