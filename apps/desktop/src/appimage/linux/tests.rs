use std::ffi::OsString;
use std::os::fd::AsRawFd as _;
use std::process::Command;

use super::*;

#[test]
fn path_loses_its_entries_inside_the_mount() {
    let path = OsString::from("/tmp/.mount_ab12/usr/bin/:/usr/local/bin:/tmp/.mount_ab12/opt:/usr/bin");

    let left = outside(&path, Path::new("/tmp/.mount_ab12")).unwrap();

    assert_eq!(left, "/usr/local/bin:/usr/bin");
}

#[test]
fn a_descriptor_the_app_inherited_stays_out_of_what_it_starts() {
    let fd = left_open_across_exec();
    assert!(inherited().contains(&fd));
    assert!(open_in_a_child(fd), "a child inherits it until it's kept out");

    let kept = leave(&[fd], InChildren::Closed);

    let in_child = open_in_a_child(fd);
    close(fd);
    assert_eq!(kept, [fd]);
    assert!(!in_child);
}

#[test]
fn what_starts_while_the_mount_is_shared_holds_it_and_nothing_after() {
    let fd = left_open_across_exec();
    leave(&[fd], InChildren::Closed);

    let during = sharing(&[fd], || open_in_a_child(fd));

    let after = open_in_a_child(fd);
    close(fd);
    assert!(during, "what the updater needs reaches it");
    assert!(!after, "and nothing started after");
}

/// A descriptor of this test's own, open across `exec`, as the `AppImage`'s runtime leaves its own, which `dup`
/// doesn't change.
fn left_open_across_exec() -> RawFd {
    let file = tempfile::tempfile().unwrap();
    // SAFETY: `dup` only reads the descriptor `file` holds, which stays open across the call.
    let fd = unsafe { libc::dup(file.as_raw_fd()) };
    assert!(fd > 2);
    fd
}

fn open_in_a_child(fd: RawFd) -> bool {
    Command::new("sh").arg("-c").arg(format!("test -e /proc/self/fd/{fd}")).status().unwrap().success()
}

fn close(fd: RawFd) {
    // SAFETY: `fd` is the test's own, from `dup`, and nothing uses it after this.
    unsafe { libc::close(fd) };
}
