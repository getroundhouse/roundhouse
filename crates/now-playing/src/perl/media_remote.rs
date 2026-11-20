//! `MediaRemote`'s calls that this crate makes, found by name in the private framework, which has no headers.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

use super::app::App;
use super::block::Block;
use crate::now_playing::NowPlaying;

const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote";

/// One of `MediaRemote`'s calls that answer, on `queue`, by calling `block` with one value.
type Ask = unsafe extern "C" fn(queue: *mut c_void, block: &Block);
/// `MRMediaRemoteSendCommand`, which sends the Now Playing app a command, with no options for those sent here.
type Send = unsafe extern "C" fn(command: u32, options: *const c_void) -> u8;

/// `MediaRemote`, loaded, with the calls this crate makes.
pub(crate) struct MediaRemote {
    /// `MRMediaRemoteGetNowPlayingApplicationPID`, whose block takes an `int`.
    app: Ask,
    /// `MRMediaRemoteGetNowPlayingApplicationIsPlaying`, whose block takes a C `bool`, taken here as the byte it is, so
    /// a value but 0 or 1 is read rather than being undefined.
    playing: Ask,
    send: Send,
    /// Where `MediaRemote` calls the blocks it's given.
    queue: *mut c_void,
}

/// The commands sent here, by the numbers `MediaRemote` has for them.
#[derive(Clone, Copy)]
pub(crate) enum Command {
    Play = 0,
    Pause = 1,
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
}

/// Loads the framework's symbols as they're first called.
const RTLD_LAZY: c_int = 1;

impl MediaRemote {
    /// `None` where the framework, or one of its calls, isn't there, such as once Apple has removed it.
    pub(crate) fn load() -> Option<Self> {
        // SAFETY: the path is a C string, and loading a system framework runs only Apple's code.
        let framework = unsafe { dlopen(FRAMEWORK.as_ptr(), RTLD_LAZY) };
        if framework.is_null() {
            return None;
        }
        let app = symbol(framework, c"MRMediaRemoteGetNowPlayingApplicationPID")?;
        let playing = symbol(framework, c"MRMediaRemoteGetNowPlayingApplicationIsPlaying")?;
        let send = symbol(framework, c"MRMediaRemoteSendCommand")?;
        // SAFETY: each is the function its name says, which takes what its type says, as MediaRemote has had since
        // macOS 10.12.
        let (app, playing, send) = unsafe {
            (
                std::mem::transmute::<*mut c_void, Ask>(app),
                std::mem::transmute::<*mut c_void, Ask>(playing),
                std::mem::transmute::<*mut c_void, Send>(send),
            )
        };
        // SAFETY: the queue of the default quality of service, which the system keeps, so it's never released.
        let queue = unsafe { dispatch_get_global_queue(0, 0) };
        Some(Self { app, playing, send, queue })
    }

    /// The Now Playing app, and whether it's playing. `None` where `MediaRemote` doesn't answer in time. A program that
    /// isn't Apple's is answered as if no app were the Now Playing app.
    pub(crate) fn now_playing(&self) -> Option<NowPlaying<App>> {
        let app = self.ask::<c_int>(self.app)?;
        let playing = self.ask::<u8>(self.playing)?;
        Some(NowPlaying { app: App::from_pid(app), playing: playing != 0 })
    }

    /// Sends the Now Playing app `command`, and returns once it has gone.
    pub(crate) fn send(&self, command: Command) {
        // SAFETY: a command MediaRemote has, and no options, which it takes as nil.
        unsafe { (self.send)(command as u32, ptr::null()) };
        // MediaRemote sends it later, so this process mustn't end first. A question asked after it is answered only
        // once it has gone, which is only a wait.
        let _ = self.ask::<c_int>(self.app);
    }

    /// Asks `call` and waits for its answer, whose block takes a `T`.
    fn ask<T: Into<i32>>(&self, call: Ask) -> Option<i32> {
        let block = Block::answering::<T>();
        // SAFETY: `call` is one of MediaRemote's that answer on a queue through a block taking a `T`, and the block is
        // never freed, so it's there however late the answer comes.
        unsafe { call(self.queue, block) };
        block.wait()
    }
}

/// The address of `name` in `framework`, or `None` where it has none.
fn symbol(framework: *mut c_void, name: &CStr) -> Option<*mut c_void> {
    // SAFETY: `framework` is a handle `dlopen` gave, and `name` a C string.
    let address = unsafe { dlsym(framework, name.as_ptr()) };
    (!address.is_null()).then_some(address)
}
