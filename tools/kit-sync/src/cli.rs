//! Runs the command line for `kit-sync`, printing how the copy or its patches were left.

use std::path::Path;

use crate::copy::COPY;
use crate::error::Error;
use crate::git::Git;
use crate::patch::{Patched, drop_patch, patch};
use crate::patches::{PATCHES, PatchName};
use crate::pin::Pin;
use crate::sync::{Synced, sync};
use crate::versions::Versions;

const USAGE: &str = "usage: kit-sync [patch <name> | drop <name>]

    kit-sync              Brings the copy and our patches to the release it pins, and moves gpui-kit and
                          gpui-pre-apple in Cargo.toml to the versions the copy has.
    kit-sync patch <name> Records our changes to the copy in third_party/gpui-kit/patches/<name>.patch,
                          putting the change's notice in each file it changes.
    kit-sync drop <name>  Takes third_party/gpui-kit/patches/<name>.patch out of the copy and deletes it.";

/// Runs `args` against the checkout at `root`, and returns whether the command finished cleanly.
pub fn run(git: &Git, root: &Path, pin: &Pin, args: &[String]) -> Result<bool, Error> {
    match args {
        [] => sync(git, root, pin).map(|synced| say_synced(pin, synced)),
        [command, name] if command == "patch" => {
            let name = PatchName::parse(name)?;
            let patched = patch(git, root, &pin.upstream, &name)?;
            say_patched(&name, &patched);
            Ok(true)
        }
        [command, name] if command == "drop" => {
            let name = PatchName::parse(name)?;
            drop_patch(git, root, &name)?;
            println!("Dropped {PATCHES}/{}.", name.file_name());
            Ok(true)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(false)
        }
    }
}

fn say_synced(pin: &Pin, synced: Synced) -> bool {
    let say_moved = |moved: Option<Versions>| {
        if let Some(Versions { kit, gpui }) = moved {
            println!("gpui-kit and gpui-pre-apple are on {kit} and {gpui} now.");
        }
    };
    match synced {
        Synced::Brought { moved, dropped } => {
            println!("The copy is on {} now.", pin.release);
            for name in dropped {
                println!("Dropped {PATCHES}/{} (now upstream).", name.file_name());
            }
            say_moved(moved);
            true
        }
        Synced::Already { moved } => {
            println!("The copy is on {} already.", pin.release);
            say_moved(moved);
            true
        }
        Synced::Unsettled { patch, files } => {
            eprintln!("Bringing {PATCHES}/{} onto {} stopped with conflicts in:", patch.file_name(), pin.release);
            for path in files {
                eprintln!("    {COPY}/{path}");
            }
            eprintln!(
                "Where both changed the same lines, git marked them in the file. Where one side deleted a file the \
                 other\nchanged, or the file isn't text, it's as one side left it, with nothing marked. Settle each \
                 in {COPY},\nthen run this again to write {} and apply any patches after it.",
                patch.file_name()
            );
            false
        }
    }
}

fn say_patched(name: &PatchName, patched: &Patched) {
    let file = format!("{PATCHES}/{}", name.file_name());
    match patched {
        Patched::Added => println!("Wrote {file}."),
        Patched::Updated => println!("Updated {file}."),
        Patched::Already => println!("{file} has those changes already."),
    }
}
