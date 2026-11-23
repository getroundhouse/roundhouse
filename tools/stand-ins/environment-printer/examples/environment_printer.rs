//! Prints the environment it was started in, as the daemon's `print-environment` role does, when it's asked the same,
//! so a hub's test can run it from a login shell without building the daemon. It prints all it was given, so a test
//! reads what the shell gave it, and nothing the stand-in chose.
#![expect(clippy::unwrap_used, reason = "a test program whose failure must fail its caller")]

/// What it exits with when it's asked anything else, so a test sees it wasn't run as the hub runs it.
const ASKED_SOMETHING_ELSE: i32 = 9;

fn main() {
    if std::env::args().nth(1).as_deref() != Some(process::PRINTS_ENVIRONMENT) {
        std::process::exit(ASKED_SOMETHING_ELSE);
    }
    process::print_environment(std::env::vars_os(), &mut std::io::stdout().lock()).unwrap();
}
