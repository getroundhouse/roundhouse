//! How long a shell `sleep` in the code can be. A script a test runs that sleeps a set time, so a process stays up while
//! the test does something, races the test on a busy machine, so a script waits on something the test makes instead.

use std::path::Path;

use super::rust_standards::{add_rust_files, covered_crates};
use super::workspace::workspace;

/// The longest sleep that's a pace for checking on something, such as in `until [ -e ready ]; do sleep 0.05; done`.
const LONGEST_POLL: f64 = 0.05;

/// The shortest sleep that's holding a process up until it's stopped, which nothing waits out.
const SHORTEST_HOLD: f64 = 30.0;

/// Each `sleep N` in `text`, as (line, N), where N is a number.
fn sleeps(text: &str) -> Vec<(usize, f64)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for (at, _) in line.match_indices("sleep ") {
            let after = &line[at + "sleep ".len()..];
            let number: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            let starts_a_word = at == 0 || !line.as_bytes()[at - 1].is_ascii_alphanumeric();
            if let (true, Ok(seconds)) = (starts_a_word, number.parse()) {
                found.push((index + 1, seconds));
            }
        }
    }
    found
}

#[test]
fn a_sleep_either_paces_a_check_or_holds_a_process_until_its_stopped() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    assert!(files.len() > 100, "found only {} files, so this isn't looking in the right place", files.len());
    let mut timed = Vec::new();
    for file in &files {
        for (line, seconds) in sleeps(&std::fs::read_to_string(file).unwrap()) {
            if seconds > LONGEST_POLL && seconds < SHORTEST_HOLD {
                timed.push(format!("{}:{line}: sleep {seconds}", file.strip_prefix(root).unwrap().display()));
            }
        }
    }
    assert!(
        timed.is_empty(),
        "a sleep of a set time races what it's meant to wait for, so wait for a file or a gate, polling at most every \
         {LONGEST_POLL}s, or hold the process with one of at least {SHORTEST_HOLD}s until it's stopped: {timed:#?}"
    );
}

#[test]
fn sleeps_are_found_wherever_a_line_has_them() {
    // Its own sleeps are spelt with an escaped space, so the file doesn't have the one it's to find.
    let text = "let a = \"sleep\x202\";\n\"until [ -e x ]; do sleep 0.05; done; sleep 60\"\nnosleep\x205; sleepy 3";
    assert_eq!(sleeps(text), [(1, 2.0), (2, 0.05), (2, 60.0)]);
}
