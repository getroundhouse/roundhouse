//! How dictation stands, in words, as the box Settings draws under the setting, and the composer's microphone opens
//! while dictation isn't ready, as `docs/spec/settings.md` says.

use std::io;

use gpui_kit::App;
use gpui_kit::assets::IconName as Lucide;
use models::{CheckError, DownloadError, FetchError};

use super::readiness::{self, Failure, Readiness};
use super::{deleting, downloading};
use crate::status_box::{Action, StatusBox, Tone};

/// The box's, and its buttons', for tests and a screen reader to find them by.
pub const STATUS: &str = "dictation-status";
pub const RETRY: &str = "dictation-retry";
pub const DELETE: &str = "dictation-delete";

/// How dictation stands, as a box, when there's something to say: what's under way, what failed, or that the model is
/// kept.
pub(crate) fn status_box(cx: &App) -> Option<StatusBox> {
    let on_disk_bytes = readiness::bytes_on_disk(cx);
    let being_deleted = readiness::deleting(cx);
    let delete =
        || Action { id: DELETE, label: "Delete the model", off: being_deleted, act: deleting::delete_the_model };
    let said = |icon, tone, lead: Option<&str>, detail: String| StatusBox {
        id: STATUS,
        icon,
        tone,
        lead: lead.map(|lead| lead.to_string().into()),
        detail: detail.into(),
        progress: None,
        action: None,
    };
    if being_deleted {
        let box_ = said(Lucide::LoaderCircle, Tone::Busy, Some("Deleting the model."), String::new());
        return Some(StatusBox { action: Some(delete()), ..box_ });
    }
    Some(match readiness::readiness(cx) {
        Readiness::Off if on_disk_bytes == 0 => return None,
        Readiness::Off => {
            let kept = format!(
                "The model is still on this Mac, {} GB, for when dictation is on again.",
                gigabytes(on_disk_bytes)
            );
            StatusBox { action: Some(delete()), ..said(Lucide::HardDrive, Tone::Quiet, None, kept) }
        }
        Readiness::Downloading => {
            let total_bytes = readiness::model_size_bytes(cx);
            // What's here, until the download says, as it does once a download that's stopping lets go of the folder.
            let done_bytes = downloading::progress(cx).map_or(on_disk_bytes.min(total_bytes), |p| p.done_bytes);
            let detail = format!("{} of {} GB.", gigabytes(done_bytes), gigabytes(total_bytes));
            let box_ = said(Lucide::Download, Tone::Busy, Some("Downloading the model."), detail);
            #[expect(clippy::cast_precision_loss, reason = "a bar's width needs nothing like 2^24 steps")]
            let done = done_bytes as f32 / total_bytes.max(1) as f32;
            StatusBox { progress: Some(done), ..box_ }
        }
        Readiness::Preparing(_) => {
            let detail = "The first time, this can take a few minutes.".to_string();
            said(Lucide::LoaderCircle, Tone::Busy, Some("Preparing the model."), detail)
        }
        Readiness::Ready(_) => {
            let detail = format!("The model is on this Mac, {} GB.", gigabytes(on_disk_bytes));
            StatusBox { action: Some(delete()), ..said(Lucide::CircleCheck, Tone::Done, Some("Ready."), detail) }
        }
        Readiness::Failed(failure) => {
            let (lead, detail) = failed(failure);
            let retry = Action { id: RETRY, label: "Retry", off: false, act: readiness::retry };
            StatusBox { action: Some(retry), ..said(Lucide::TriangleAlert, Tone::Problem, Some(lead), detail) }
        }
        Readiness::NeedsNewerMacos => {
            let detail = format!("It needs macOS {} or later.", dictation::nemotron::OLDEST_MACOS);
            said(Lucide::TriangleAlert, Tone::Problem, Some("Dictation can't run on this Mac."), detail)
        }
    })
}

/// What failed, as the step that did, and why, in a few words.
fn failed(failure: &Failure) -> (&'static str, String) {
    match failure {
        Failure::Download(e) => ("Couldn't download the model:", why_it_didnt_come(e)),
        Failure::Start(e) => ("Couldn't start dictation:", format!("{e}.")),
        Failure::Prepare(e) => ("Couldn't prepare the model:", format!("{e}.")),
    }
}

fn why_it_didnt_come(download: &DownloadError) -> String {
    let fetch = match download {
        DownloadError::Folder(folder) => return not_saved(&folder.source),
        DownloadError::Check(CheckError::NotTheFile { .. }) => return "what came wasn't the file it should be.".into(),
        DownloadError::Check(CheckError::Fetch(fetch)) => fetch,
    };
    if fetch.timed_out() {
        return "the connection stalled.".to_string();
    }
    match fetch {
        FetchError::Request { .. } => "Hugging Face couldn't be reached.".to_string(),
        FetchError::Status { status, .. } => format!("Hugging Face answered {status}."),
        FetchError::Read { .. } | FetchError::CutShort { .. } => "the connection was lost.".to_string(),
        FetchError::TooLong { .. } => "what came wasn't the file it should be.".to_string(),
        FetchError::Save { source, .. } => not_saved(source),
    }
}

fn not_saved(source: &io::Error) -> String {
    if source.kind() == io::ErrorKind::StorageFull {
        return "there isn't room for it on this Mac.".to_string();
    }
    format!("it couldn't be saved: {source}.")
}

/// `bytes` in gigabytes, to a tenth.
pub(crate) fn gigabytes(bytes: u64) -> String {
    let tenths = (bytes + 50_000_000) / 100_000_000;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{gigabytes, not_saved};

    #[test]
    fn gigabytes_are_to_a_tenth() {
        assert_eq!(gigabytes(dictation::nemotron::MODEL.size_bytes()), "2.4");
        assert_eq!(gigabytes(0), "0.0");
        assert_eq!(gigabytes(1_249_999_999), "1.2");
    }

    #[test]
    fn a_full_disk_says_so() {
        assert_eq!(not_saved(&io::Error::from(io::ErrorKind::StorageFull)), "there isn't room for it on this Mac.");
    }
}
