//! `kit-sync` brings `third_party/gpui-kit/vendor` and our patches in `third_party/gpui-kit/patches` to the release it
//! pins, and records or drops patches. `third_party/README.md` says when to run each command.

use std::path::Path;
use std::process::ExitCode;

use kit_sync::{Git, Pin, run};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // This is run from a checkout, by `cargo run -p kit-sync`, so the checkout it was built in is the one it keeps.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("couldn't find the repository at {}: {error}", root.display());
            return ExitCode::FAILURE;
        }
    };
    match run(&Git::new(std::env::vars_os()), &root, &Pin::ours(), &args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
