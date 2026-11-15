//! The programs a command can be drawn as: those that do nothing but print a file, list files or search what's in
//! them, given only what's known to do nothing more. A program given anything else, such as a flag this doesn't know or
//! more files than one part names, does more than that, or does it to more than a part can say. Each is named as the
//! shell finds it on the path, since a path to a program, such as `./cat`, can be any program.

use protocol::capability::calls::call::CommandPart;

use super::flags::{ANY, GIT_GREP, GIT_LS_FILES, GREP, HEAD, LS, NL, RG, TAIL, given};
use super::words::Words;

/// What `program` does, where it does nothing but print one file, list files in one place at most, or search what's in
/// files for one pattern in one place at most.
pub(super) fn part(program: &Words) -> Option<CommandPart> {
    let words: Vec<&str> = program.iter().map(String::as_str).collect();
    read(&words).or_else(|| list(&words)).or_else(|| search(&words))
}

/// Whether `words` is `sed -n` printing a line, or a range of them, such as `1,40p`, of what's piped into it, which
/// only cuts that short.
pub(super) fn prints_lines(words: &Words) -> bool {
    matches!(words.as_slice(), [sed, n, range] if sed == "sed" && n == "-n" && is_range(range))
}

fn read(words: &[&str]) -> Option<CommandPart> {
    let file = match words {
        ["sed", "-n", range, file] => is_range(range).then_some(*file),
        ["cat", rest @ ..] => given(rest, &ANY)?.one_operand(),
        ["head", rest @ ..] => given(rest, &HEAD)?.one_operand(),
        ["tail", rest @ ..] => given(rest, &TAIL)?.one_operand(),
        ["nl", rest @ ..] => given(rest, &NL)?.one_operand(),
        _ => None,
    };
    Some(CommandPart::Read { path: a_file(file?)?.to_owned() })
}

fn list(words: &[&str]) -> Option<CommandPart> {
    let given = match words {
        ["ls", rest @ ..] => given(rest, &LS)?,
        ["git", "ls-files", rest @ ..] => given(rest, &GIT_LS_FILES)?,
        ["rg", rest @ ..] => given(rest, &RG).filter(|given| given.has("--files"))?,
        _ => return None,
    };
    if !given.patterns.is_empty() {
        return None;
    }
    let path = match given.operands.as_slice() {
        [] => None,
        [path] => Some(a_file(path)?.to_owned()),
        _ => return None,
    };
    Some(CommandPart::List { path })
}

fn search(words: &[&str]) -> Option<CommandPart> {
    let (given, revisions) = match words {
        ["grep" | "egrep" | "fgrep", rest @ ..] => (given(rest, &GREP)?, false),
        ["rg", rest @ ..] => (given(rest, &RG).filter(|given| !given.has("--files"))?, false),
        ["git", "grep", rest @ ..] => (given(rest, &GIT_GREP)?, true),
        _ => return None,
    };
    // Without a flag that gives it, the pattern is the first word that isn't a flag. `git grep` takes it only before a
    // `--`, and what follows it there can be a revision to search rather than a place.
    let (pattern, from) = match given.patterns.as_slice() {
        [] => (Some(*given.operands.first().filter(|_| !revisions || given.before_dashes > 0)?), 1),
        [pattern] => (*pattern, 0),
        _ => return None,
    };
    let path = match &given.operands[from..] {
        [] => None,
        [path] if !revisions || from >= given.before_dashes => Some(a_file(path)?.to_owned()),
        _ => return None,
    };
    Some(CommandPart::Search { pattern: pattern.map(str::to_owned), path })
}

/// `word`, where it names a file or a place, rather than what's piped in, as `-` does.
fn a_file(word: &str) -> Option<&str> {
    Some(word).filter(|word| !word.is_empty() && *word != "-")
}

/// `40p` or `1,40p`, a line or a range of them to print.
fn is_range(script: &str) -> bool {
    let Some(lines) = script.strip_suffix('p') else { return false };
    let number = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    match lines.split_once(',') {
        Some((first, last)) => number(first) && number(last),
        None => number(lines),
    }
}
