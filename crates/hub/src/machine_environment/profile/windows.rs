//! The profile's environment on Windows, which has no login shell to ask: the user's and the system's variables in the
//! registry, as Explorer has them at logon.

use std::ffi::c_void;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_QUERY};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::super::Vars;
use super::entries::from_block;

/// The environment Windows makes for this process's user from the registry, with each value expanded and the system's
/// `Path` and the user's joined, as it makes the one Explorer starts with. Neither a shell, `session` nor a printer is
/// needed, since the block has the session's variables too. `None` if Windows couldn't make it.
pub(in crate::machine_environment) async fn read(
    _shell: Option<&str>,
    _session: &Vars,
    _printer: &Path,
) -> Option<Vars> {
    tokio::task::spawn_blocking(users_block).await.ok().flatten()
}

fn users_block() -> Option<Vars> {
    let token = own_token()?;
    let mut block: *mut c_void = std::ptr::null_mut();
    // SAFETY: `block` is somewhere for the pointer to go, and `token` is a token of this process's, open with the access
    // the call asks for. `FALSE` asks for none of this process's own environment.
    let made = unsafe { CreateEnvironmentBlock(&raw mut block, token, 0) };
    // SAFETY: `token` was opened by `own_token`, and nothing uses it after this.
    unsafe { CloseHandle(token) };
    if made == 0 || block.is_null() {
        return None;
    }
    // SAFETY: Windows made `block` as UTF-16 variables, each ended by a NUL, and the last by a second, so reading up to
    // and including that stays inside it. It's copied before it's destroyed, once.
    let units = unsafe {
        let start = block.cast::<u16>();
        let mut len = 0;
        while *start.add(len) != 0 || *start.add(len + 1) != 0 {
            len += 1;
        }
        let units = std::slice::from_raw_parts(start, len + 1).to_vec();
        DestroyEnvironmentBlock(block);
        units
    };
    Some(from_block(&units))
}

/// This process's token, open to make an environment block for its user, as `CreateEnvironmentBlock` asks of a primary
/// token.
fn own_token() -> Option<HANDLE> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current process's pseudo handle needs no closing, and `token` is somewhere for the handle to go.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE, &raw mut token) };
    (opened != 0).then_some(token)
}
