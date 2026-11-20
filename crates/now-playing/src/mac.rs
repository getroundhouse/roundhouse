//! The part of pausing on a Mac that runs in the app: starting `/usr/bin/perl` with the library the build script makes
//! from this crate, which pauses the Now Playing app and plays it again itself, in that order, as `src/perl` says. Perl
//! only loads the library and calls it.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Whether this platform can pause what's playing while it's dictated to. A Mac can, through a private framework, so
/// it can fail as Apple changes the Mac, and then pauses nothing.
pub const PAUSES: bool = true;

/// Perl, holding a pause until [`play_again`], whether or not anything was playing to pause.
pub struct Paused(Child);

/// Where the build script made the library, for a build run from a checkout. A package has it beside the binaries
/// instead, under the same name, where `tools/build.sh` puts it.
const BUILT: &str = env!("ROUNDHOUSE_NOW_PLAYING");

/// The library's function perl calls.
const FUNCTION: &str = "roundhouse_pause_while_held";

/// Loads the library its first argument names into perl and calls the function its second names, failing where
/// either isn't there.
const LOAD: &str = "use DynaLoader; my ($library, $function) = @ARGV; \
    my $handle = DynaLoader::dl_load_file($library, 0) or die DynaLoader::dl_error(); \
    my $symbol = DynaLoader::dl_find_symbol($handle, $function) or die DynaLoader::dl_error(); \
    DynaLoader::dl_install_xsub('main::pause', $symbol); pause();";

/// Starts perl pausing the Now Playing app, if it's playing, without waiting for it, until [`play_again`]. `None` where
/// there's no library, or perl didn't start. Perl's own failures, such as once Apple stops it loading the library, end
/// it, which pauses nothing.
pub fn pause() -> Option<Paused> {
    Command::new("/usr/bin/perl")
        .arg("-e")
        .arg(LOAD)
        .arg(library()?)
        .arg(FUNCTION)
        // Nothing of the app's, such as `PERL5OPT`, changes what perl does.
        .env_clear()
        // Written to by `play_again`, and closed once the app ends, however it ends.
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // So Control-C in the terminal that ran the app doesn't end it too, before it plays again.
        .process_group(0)
        .spawn()
        .ok()
        .map(Paused)
}

/// Has perl play again what it paused, if anything, and waits until it has, or has ended.
pub fn play_again(Paused(mut perl): Paused) {
    if let Some(mut input) = perl.stdin.take() {
        // A byte says to, as the end of the input does. The end alone could be held up: std on a Mac makes the pipe
        // before marking it to close as other programs start, so one another thread starts at that moment would keep
        // it open. Fails where perl has ended already, having paused nothing.
        let _ = input.write_all(b"\n");
    }
    // Every wait in the library has a limit, once it has its byte, so this returns.
    let _ = perl.wait();
}

/// The library beside this program, as a package has it, or else where this build made it.
fn library() -> Option<PathBuf> {
    let built = Path::new(BUILT);
    let beside =
        std::env::current_exe().ok().zip(built.file_name()).map(|(program, name)| program.with_file_name(name));
    beside.into_iter().chain([built.to_path_buf()]).find(|library| library.is_file())
}

#[cfg(test)]
mod tests {
    use std::ffi::{CString, c_char, c_int, c_void};
    use std::os::unix::ffi::OsStrExt as _;

    use super::{FUNCTION, library};

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    /// Finds the function without calling it, since calling it would pause whatever plays on the machine running the
    /// test.
    #[test]
    fn the_build_makes_the_library_with_the_function_perl_calls() {
        let library = CString::new(library().unwrap().as_os_str().as_bytes()).unwrap();
        let function = CString::new(FUNCTION).unwrap();

        // SAFETY: a C string naming the library the build script made from this crate, which runs nothing as it loads
        // but the standard library's own setup.
        let handle = unsafe { dlopen(library.as_ptr(), 1) };
        assert!(!handle.is_null(), "{library:?} doesn't load");
        // SAFETY: a handle `dlopen` gave, and a C string.
        let symbol = unsafe { dlsym(handle, function.as_ptr()) };

        assert!(!symbol.is_null(), "{library:?} has no {FUNCTION}");
    }
}
