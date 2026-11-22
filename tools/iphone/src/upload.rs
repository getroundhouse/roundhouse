//! Uploading the app to `TestFlight`, and seeing it through App Store Connect's processing. An upload that `xcodebuild`
//! says succeeded can still be rejected by the processing after it, which only Apple's email would say otherwise.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::Context as _;

use crate::cargo::{self, Profile};
use crate::connect::{self, Client, Key, Notes};
use crate::stamp::{BuildNumber, Version};
use crate::xcode::{self, App};

/// How long processing may take before the upload counts as failed. It has taken about five minutes.
const PROCESSING_WITHIN: Duration = Duration::from_mins(30);

/// Uploads `binary`, or one built now, as this commit's build, waits for App Store Connect to process it, and gives it
/// `notes`' text as what testers should try.
pub(crate) fn upload(
    root: &Path,
    app: &App,
    key: &Key,
    binary: Option<&Path>,
    notes: Option<&Path>,
) -> anyhow::Result<()> {
    let client = Client::new(key);
    // Before the slow part, so a missing record or a key App Store Connect refuses fails at once.
    let record = connect::find_app(&client, &app.bundle)?;
    let notes = match notes {
        Some(path) => {
            Some(Notes::new(&fs::read_to_string(path).with_context(|| format!("couldn't read {}", path.display()))?))
        }
        None => None,
    };
    let binary = match binary {
        Some(binary) => binary.to_path_buf(),
        None => cargo::compile(root, Profile::Release)?,
    };
    let version = Version::of(root)?;
    let build = BuildNumber::at(jiff::Timestamp::now());
    let work = root.join("apps/mobile-ios/build/testflight");
    if work.exists() {
        fs::remove_dir_all(&work).with_context(|| format!("couldn't remove {}", work.display()))?;
    }
    fs::create_dir_all(&work).with_context(|| format!("couldn't make {}", work.display()))?;

    let archive = xcode::archive(root, app, &binary, (&version, &build), &work)?;
    xcode::export_and_upload(&archive, &app.team, key, &work)?;
    println!("Uploaded {version} ({build}) to App Store Connect");

    let processed = connect::wait_for_processing(&client, &record, &build, PROCESSING_WITHIN)?;
    println!("App Store Connect processed it, so TestFlight offers it to the team's testers");
    if let Some(notes) = notes {
        connect::set_notes(&client, &record, &processed, &notes)?;
        println!("Told testers what to try");
    }
    Ok(())
}
