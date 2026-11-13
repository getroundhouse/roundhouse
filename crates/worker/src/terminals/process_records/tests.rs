// On Unix, which has groups to kill.
#![cfg(unix)]

use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

use super::keeping::*;

const WAIT: Duration = Duration::from_secs(10);
const SIGKILL: i32 = 9;

/// A process leading a group of its own, as a shell or the job it runs in front does.
fn a_group() -> Child {
    let mut command = Command::new("sleep");
    command.arg("60");
    process::own_group(&mut command);
    command.spawn().unwrap()
}

fn recorded(dir: &Path) -> Vec<String> {
    let names = std::fs::read_dir(dir).unwrap().flatten();
    let mut names: Vec<String> = names.map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn a_terminal_that_finishes_with_its_records_removes_them_and_kills_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut shell, mut front) = (a_group(), a_group());
    let mut records = ProcessRecords::new(dir.path(), &"t".into());

    records.shell(shell.id());
    records.front(Some(front.id()));
    assert_eq!(recorded(dir.path()), ["terminal-t.front.pid", "terminal-t.pid"]);
    records.finished();

    assert_eq!(recorded(dir.path()), Vec::<String>::new());
    for child in [&mut shell, &mut front] {
        assert!(child.try_wait().unwrap().is_none(), "it was killed");
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// As when the terminal's thread unwinds, which leaves nothing else to end them.
#[test]
fn a_terminal_that_lets_go_of_its_records_unfinished_kills_what_they_record() {
    let dir = tempfile::tempdir().unwrap();
    let (mut shell, mut front) = (a_group(), a_group());
    let mut records = ProcessRecords::new(dir.path(), &"t".into());

    records.shell(shell.id());
    records.front(Some(front.id()));
    drop(records);

    assert_eq!(recorded(dir.path()), Vec::<String>::new());

    for child in [&mut shell, &mut front] {
        let ended = waiting::thread::until_ok(WAIT, || child.try_wait().unwrap().ok_or("it was never killed"));
        assert_eq!(ended.signal(), Some(SIGKILL));
    }
}
