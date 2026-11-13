//! What stands in for process groups and `ps` on Windows: a process's descendants, found from each process's parent,
//! and its start time, both through the Win32 API. Also which of a process's handles its children get.
//!
//! A descendant whose own parent has exited isn't found, since nothing links it to the tree any more. A job object
//! would hold it.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::process::Command;

use windows_sys::Win32::Foundation::{
    FILETIME, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, GetProcessTimes, OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

use crate::Signal;

/// The System Idle Process is 0 and System is 4, and both are the parent of much that isn't theirs to end.
const LAST_SYSTEM_PID: u32 = 4;

/// What a process that's ended this way exits with, since Windows records no signal.
const ENDED_EXIT_CODE: u32 = 1;

/// How long ending a process waits for it to be gone. Windows only starts ending it when asked, and it almost always
/// goes within a few milliseconds. One that takes longer is left to finish, so a stop is never held up for long.
const ENDING_WAIT_MS: u32 = 200;

/// A process held open, so its PID can't go to another process meanwhile.
struct Process {
    pid: u32,
    handle: OwnedHandle,
    /// In 100 ns ticks since 1601, as Windows counts.
    started: u64,
}

impl Process {
    /// `None` if there's no such process, it has exited, or this user may not open it.
    fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<Self> {
        // SAFETY: OpenProcess takes any PID, and returns a new handle or null.
        let raw = unsafe { OpenProcess(access | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is an open handle that nothing else owns.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        // A process that has exited can still be opened while anything holds a handle to it, as whatever started
        // it does, and it's only signalled once it has exited.
        // SAFETY: `handle` is open, with the right to wait on it.
        if unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } != WAIT_TIMEOUT {
            return None;
        }
        let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: `handle` is open, with the right to query it, and each pointer is to a FILETIME that outlives
        // the call.
        if unsafe { GetProcessTimes(handle.as_raw_handle(), created, exited, kernel, user) } == 0 {
            return None;
        }
        let started = u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime);
        Some(Self { pid, handle, started })
    }

    /// Ends it, and waits a moment for it to be gone, since until then it can still start another process.
    fn end(&self) {
        // SAFETY: `handle` is open, with the rights to terminate and to wait, and neither call touches memory.
        unsafe {
            TerminateProcess(self.handle.as_raw_handle(), ENDED_EXIT_CODE);
            WaitForSingleObject(self.handle.as_raw_handle(), ENDING_WAIT_MS);
        }
    }
}

/// Only Ctrl-C and Ctrl-Break know about a group on Windows, and this keeps the console's Ctrl-C from it.
pub(crate) fn own_group(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// Every signal ends the leader and what it started. It doesn't ask first with Ctrl-Break, the nearest thing Windows
/// has to `SIGTERM`.
pub(crate) fn signal_group(leader: u32, signal: Signal) {
    match signal {
        Signal::Hangup | Signal::Interrupt | Signal::Terminate | Signal::Kill => end_tree(leader),
    }
}

pub(crate) fn keep_stdio_from_children() {
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle takes any of the three and touches no memory. It returns a handle, or null or
        // INVALID_HANDLE_VALUE for a process without that one.
        keep_from_children(unsafe { GetStdHandle(which) });
    }
}

/// Stops the children this process starts from here on getting a copy of `handle`. Anything that isn't an open
/// handle, such as null, is left as it is.
fn keep_from_children(handle: HANDLE) {
    // SAFETY: SetHandleInformation takes any value as a handle, failing for one that isn't, and touches no memory.
    unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
}

/// In the system's own count, as a number.
pub(crate) fn start_time(pid: u32) -> Option<String> {
    Process::open(pid, 0).map(|process| process.started.to_string())
}

/// Ends `leader` and everything it started. It looks again after each round, because a process can start another
/// before it's gone. Holding each one open keeps its PID from being reused, so a process naming it as its parent
/// really is its child, as long as it started later.
fn end_tree(leader: u32) {
    if leader <= LAST_SYSTEM_PID {
        return;
    }
    let Some(leader) = Process::open(leader, PROCESS_TERMINATE) else {
        return;
    };
    leader.end();
    let mut tree = vec![leader];
    let mut ended = 0;
    while ended < tree.len() {
        ended = tree.len();
        for (pid, parent) in parents() {
            let started_by = tree.iter().find(|process| process.pid == parent).map(|process| process.started);
            if let Some(parent_started) = started_by
                && tree.iter().all(|process| process.pid != pid)
                && let Some(child) = Process::open(pid, PROCESS_TERMINATE)
                && child.started >= parent_started
            {
                child.end();
                tree.push(child);
            }
        }
    }
}

/// Every running process's PID with its parent's, parents before the children they started where Windows lists
/// them so. A parent's PID stays on its children after it exits, and can then go to an unrelated process.
fn parents() -> Vec<(u32, u32)> {
    // SAFETY: the call takes no pointers, and returns a new handle or INVALID_HANDLE_VALUE.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    // SAFETY: `raw` is an open handle that nothing else owns.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: PROCESSENTRY32W is integers and an array of them, for which all zeroes is a value.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or_default();
    let mut found = Vec::new();
    // SAFETY: `snapshot` is open, and `entry` outlives each call and says its own size, as they require.
    let mut more = unsafe { Process32FirstW(snapshot.as_raw_handle(), &raw mut entry) };
    while more != 0 {
        found.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as for the first.
        more = unsafe { Process32NextW(snapshot.as_raw_handle(), &raw mut entry) };
    }
    found
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader, Read as _};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use super::*;

    fn children_of(pid: u32) -> Vec<u32> {
        parents().into_iter().filter(|(_, parent)| *parent == pid).map(|(child, _)| child).collect()
    }

    #[test]
    fn ending_a_tree_reaches_what_the_leader_started() {
        let mut command = Command::new("cmd");
        command.args(["/c", "ping -n 60 127.0.0.1"]).stdout(Stdio::piped());
        own_group(&mut command);
        let mut leader = command.spawn().unwrap();
        // Once `ping` has printed, it's running. A console's own process can be the leader's child too, so every
        // child counts, rather than whichever comes first.
        let mut line = String::new();
        BufReader::new(leader.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let started = children_of(leader.id());
        assert!(!started.is_empty(), "the leader has started nothing, after printing {line:?}");

        signal_group(leader.id(), Signal::Terminate);

        assert_eq!(leader.wait().unwrap().code(), i32::try_from(ENDED_EXIT_CODE).ok());
        let deadline = Instant::now() + Duration::from_secs(5);
        while started.iter().any(|child| start_time(*child).is_some()) {
            assert!(Instant::now() < deadline, "what the leader started is still running");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_pipe_kept_from_children_ends_when_its_owner_lets_go() {
        let (mut reader, writer) = std::io::pipe().unwrap();
        // Passed on to children, as a process's own output is when it was started with it.
        // SAFETY: `writer` is an open handle, and the call touches no memory.
        unsafe { SetHandleInformation(writer.as_raw_handle(), HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
        keep_from_children(writer.as_raw_handle());
        // Given none of the pipe, and still running after the pipe's owner lets go, as what an agent left behind is.
        let mut command = Command::new("cmd");
        command.args(["/c", "ping -n 30 127.0.0.1"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        own_group(&mut command);
        let mut started = command.spawn().unwrap();

        drop(writer);

        let (ended, end) = std::sync::mpsc::channel();
        std::thread::spawn(move || ended.send(reader.read_to_end(&mut Vec::new()).is_ok()));
        let read = end.recv_timeout(Duration::from_secs(10));
        signal_group(started.id(), Signal::Kill);
        started.wait().unwrap();
        assert_eq!(read, Ok(true), "what this process started still holds the pipe");
    }

    #[test]
    fn the_system_and_a_pid_nothing_has_are_left_alone() {
        signal_group(0, Signal::Kill);
        signal_group(4, Signal::Kill);
        signal_group(u32::MAX, Signal::Kill);
    }
}
