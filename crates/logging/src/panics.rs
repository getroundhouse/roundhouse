//! Logging a panic.

use std::panic::{self, PanicHookInfo};

/// Logs each panic from now on, with its message and where it happened, then prints it as before. The message is the one
/// thing a log holds that can quote what the code was working on, as `docs/principles/observability.md` has it.
pub(crate) fn log_each() {
    let print = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        log(panic);
        print(panic);
    }));
}

fn log(panic: &PanicHookInfo<'_>) {
    let at = panic.location().map(ToString::to_string);
    let thread = std::thread::current();
    let backtrace = std::backtrace::Backtrace::force_capture();
    let message = panic.payload_as_str();
    tracing::error!(
        name: "process.panicked",
        panic = message,
        at,
        thread = thread.name(),
        %backtrace,
        "Panicked"
    );
}
