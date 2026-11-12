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
    let file = tempfile::tempfile().unwrap();
    // As the AppImage's runtime leaves its own, open across `exec`, which `dup` doesn't change.
    // SAFETY: `dup` only reads the descriptor `file` holds, which stays open across the call.
    let fd = unsafe { libc::dup(file.as_raw_fd()) };
    assert!(fd > 2);
    let open_in_a_child = || Command::new("sh").arg("-c").arg(format!("test -e /proc/self/fd/{fd}")).status().unwrap();
    assert!(inherited().contains(&fd));
    assert!(open_in_a_child().success(), "a child inherits it until it's kept out");

    keep_from_children([fd]);

    let in_child = open_in_a_child();
    // SAFETY: `fd` is this test's own, from `dup`, and nothing uses it after this.
    unsafe { libc::close(fd) };
    assert!(!in_child.success());
}
