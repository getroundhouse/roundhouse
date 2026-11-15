//! `kit-sync` brings `third_party/gpui-kit` to the release it pins, merged with our changes, and `kit-sync diff
//! [PATH…]` prints our changes to it, or to the files named from it, as a patch to offer upstream. The library's
//! docs say how each works, and `third_party/README.md` when to run them.

use std::path::Path;
use std::process::ExitCode;

use kit_sync::{COPY, Git, Pin, Synced, Versions};

const USAGE: &str = "usage: kit-sync [diff [PATH…]]

    kit-sync              Brings the copy to the release it pins, merged with our changes, and moves gpui-kit and
                          gpui-pre-apple in Cargo.toml to the versions the copy has.
    kit-sync diff [PATH…] Prints our changes to the copy, or to the files named from it, as a patch against the
                          release it's on, from the root of upstream's repository, without the lines that say a file
                          was changed.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let paths: Vec<String> = args.collect();
    // This is run from a checkout, by `cargo run -p kit-sync`, so the checkout it was built in is the one it keeps.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("couldn't find the repository at {}: {error}", root.display());
            return ExitCode::FAILURE;
        }
    };
    let git = Git::new(std::env::vars_os());
    let pin = Pin::ours();
    let done = match command.as_deref() {
        None => kit_sync::sync(&git, &root, &pin).map(|synced| said(&pin, synced)),
        Some("diff") => kit_sync::diff(&git, &root, &pin.upstream, &paths).map(|patch| {
            print!("{patch}");
            true
        }),
        Some(_) => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match done {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Says how the copy was left, and whether that's done.
fn said(pin: &Pin, synced: Synced) -> bool {
    let moved = |moved: Option<Versions>| {
        if let Some(Versions { kit, gpui }) = moved {
            println!("gpui-kit and gpui-pre-apple are on {kit} and {gpui} now.");
        }
    };
    match synced {
        Synced::Brought { moved: versions } => {
            println!("The copy is on {} now.", pin.release);
            moved(versions);
            true
        }
        Synced::Already { moved: versions } => {
            println!("The copy is on {} already.", pin.release);
            moved(versions);
            true
        }
        Synced::Unsettled(conflicts) => {
            eprintln!("The copy is on {}, but upstream and we both changed these:", pin.release);
            for path in conflicts {
                eprintln!("    {COPY}/{path}");
            }
            eprintln!(
                "Where both changed the same lines, git marked them in the file. Where one side deleted a file the \
                 other\nchanged, or the file isn't text, it's as one side left it, with nothing marked. Settle each, \
                 then run\nthis again, which checks that no marks are left and moves the versions in Cargo.toml."
            );
            false
        }
    }
}
