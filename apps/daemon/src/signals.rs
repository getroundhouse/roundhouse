//! The signals this process takes, which it can be started without.

/// Unblocks every signal, first in `main`. Blocked signals carry across fork and exec, Rust's `Command` no longer
/// resets them, and the desktop app starts a hub from a background thread that has them all blocked. This binary needs
/// them: tokio only learns that a child exited from `SIGCHLD`, so without it the first `git` never returns, and
/// `SIGTERM` is how it's asked to stop. Everything this starts inherits the result, agents included.
#[cfg(unix)]
pub(crate) fn unblock() {
    let mut none = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: `sigemptyset` initializes the set it's given, and this runs on the main thread before any
    // other exists, so every thread started later inherits the empty mask.
    unsafe {
        libc::sigemptyset(none.as_mut_ptr());
        libc::pthread_sigmask(libc::SIG_SETMASK, none.as_ptr(), std::ptr::null_mut());
    }
}

/// Nothing on Windows, which has no signals to block.
#[cfg(windows)]
pub(crate) fn unblock() {}
