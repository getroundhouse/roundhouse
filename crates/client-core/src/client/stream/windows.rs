//! A named pipe that can be read while it's written to. Windows runs a pipe's reads and writes one at a time unless
//! it's opened for overlapped I/O, so a read waiting for the hub would hold up the request the hub is waiting for.
//! With overlapped I/O each one is started, and then waited for on an event of its own.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, ERROR_PIPE_BUSY, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OVERLAPPED, ReadFile, SECURITY_IDENTIFICATION, WriteFile};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows_sys::core::BOOL;

/// A hub makes its pipe's next instance as soon as a client takes one, so being busy passes in well under this,
/// unless the hub has stopped listening.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

pub(in crate::client) struct Stream {
    pipe: Arc<Pipe>,
    /// Set when this clone's read or write finishes. Each clone has its own, since one reads while another writes.
    finished: OwnedHandle,
}

struct Pipe {
    file: File,
    /// Set by `shutdown` and never reset, so it ends whatever is under way and whatever is tried later.
    shut_down: OwnedHandle,
}

impl Stream {
    /// The hub may only learn who this is, not act as them, in case something else took the pipe's name first.
    pub(in crate::client) fn connect(path: &Path) -> io::Result<Self> {
        let deadline = Instant::now() + BUSY_TIMEOUT;
        let file = loop {
            let opened = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_OVERLAPPED)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(path);
            match opened {
                // Every instance of the pipe has a client, until the hub makes the next.
                Err(e) if e.raw_os_error() == i32::try_from(ERROR_PIPE_BUSY).ok() => {
                    if Instant::now() >= deadline {
                        // The hub is shutting down, and only its last clients keep the pipe. A socket would be
                        // gone by now, so this says what a client takes to mean the same.
                        return Err(io::Error::new(io::ErrorKind::ConnectionRefused, e));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                opened => break opened?,
            }
        };
        Ok(Self { pipe: Arc::new(Pipe { file, shut_down: event()? }), finished: event()? })
    }

    pub(in crate::client) fn try_clone(&self) -> io::Result<Self> {
        Ok(Self { pipe: self.pipe.clone(), finished: event()? })
    }

    /// Ends reads and writes on every clone, whichever is asked for, as a client only ever asks for both.
    pub(in crate::client) fn shutdown(&self, _how: std::net::Shutdown) -> io::Result<()> {
        // SAFETY: `shut_down` is an open event, and the call has no memory effects.
        if unsafe { SetEvent(self.pipe.shut_down.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Starts a read or a write with `start` and waits for it to finish, or for the stream to be shut down, which
    /// cancels it. Returns how many bytes it moved.
    ///
    /// # Safety
    ///
    /// The buffer `start` hands to Windows must stay valid, and unused by anything else, until this returns. Windows
    /// keeps using it and the `OVERLAPPED` until the operation finishes, which is why every path below that started
    /// one waits for it.
    unsafe fn transfer(&self, start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> BOOL) -> io::Result<usize> {
        let pipe = self.pipe.file.as_raw_handle();
        // A write the pipe has room for finishes at once, so without this it would go through after a shutdown.
        // SAFETY: `shut_down` is an open event, and the call has no memory effects.
        if unsafe { WaitForSingleObject(self.pipe.shut_down.as_raw_handle(), 0) } != WAIT_TIMEOUT {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        // SAFETY: OVERLAPPED is integers and pointers, for which all zeroes is a value.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = self.finished.as_raw_handle();
        if start(pipe, &raw mut overlapped) == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != i32::try_from(ERROR_IO_PENDING).ok() {
                // It never started, so nothing is using the buffer.
                return Err(error);
            }
            let either = [self.finished.as_raw_handle(), self.pipe.shut_down.as_raw_handle()];
            // SAFETY: `either` holds two open events, and outlives the call.
            let woken_by = unsafe { WaitForMultipleObjects(2, either.as_ptr(), 0, INFINITE) };
            if woken_by != WAIT_OBJECT_0 {
                // SAFETY: `pipe` is open, and `overlapped` is the operation started on it above.
                unsafe { CancelIoEx(pipe, &raw const overlapped) };
            }
        }
        let mut moved = 0;
        // Waits, so a cancelled operation has finished too before the buffer and `overlapped` go.
        // SAFETY: `pipe` is open, `overlapped` is the operation started on it above, and `moved` outlives the call.
        if unsafe { GetOverlappedResult(pipe, &raw const overlapped, &raw mut moved, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(usize::try_from(moved).unwrap_or_default())
    }
}

/// An event that stays set until it's reset, and starts unset.
fn event() -> io::Result<OwnedHandle> {
    // SAFETY: both pointers may be null, for the default security and no name. Returns a new handle or null.
    let raw = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is an open handle that nothing else owns.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// As much of a buffer as one call to Windows can take.
fn length(buffer: &[u8]) -> u32 {
    u32::try_from(buffer.len()).unwrap_or(u32::MAX)
}

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let (start, len) = (buffer.as_mut_ptr(), length(buffer));
        // SAFETY: `buffer` is borrowed for this whole call, and Windows writes at most `len` bytes into it.
        let read =
            unsafe { self.transfer(|pipe, overlapped| ReadFile(pipe, start, len, std::ptr::null_mut(), overlapped)) };
        match read {
            // How a pipe says its other end has closed.
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(0),
            read => read,
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let (start, len) = (buffer.as_ptr(), length(buffer));
        // SAFETY: `buffer` is borrowed for this whole call, and Windows reads at most `len` bytes from it.
        unsafe { self.transfer(|pipe, overlapped| WriteFile(pipe, start, len, std::ptr::null_mut(), overlapped)) }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
