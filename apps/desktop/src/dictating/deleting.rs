//! Deleting the model, from the box under Dictation in Settings, which turns dictation off first, and has it follow
//! the person's choice again once the files are gone.

use std::io;

use dictation::model::{self, FolderError};
use gpui_kit::{App, AsyncApp};

use super::DictationChoice;
use super::readiness::{dictating, dictating_mut, follow_the_choice, reread_whats_on_disk};

/// Why the model couldn't all be deleted.
#[derive(Debug, thiserror::Error)]
enum NotDeleted {
    #[error("the thread to delete it on couldn't start: {0}")]
    Start(io::Error),
    #[error(transparent)]
    Folder(#[from] FolderError),
    #[error("the thread deleting it stopped")]
    Stopped,
}

/// Deletes every revision of the model's files. Dictation is turned off first if it's on, so the thread lets go of the
/// model, and it follows the choice again once the files are gone, so turning it on meanwhile downloads them again only
/// then. A download that's stopping, after dictation was turned off, finishes its read first, since the files' folder
/// is one download or deletion at a time.
pub(super) fn delete_the_model(cx: &mut App) {
    if dictating(cx).deleting.is_some() {
        return;
    }
    let model_dir = dictating(cx).model.dir.clone();
    if crate::settings::get(cx).dictation == DictationChoice::On {
        crate::settings::change(cx, |settings| settings.dictation = DictationChoice::Off);
        follow_the_choice(cx);
    }
    let (tell, told) = flume::bounded(1);
    let thread = std::thread::Builder::new().name("dictation-delete".to_string());
    if let Err(e) = thread.spawn(move || drop(tell.send(model::delete(&model_dir)))) {
        return deleted(Err(NotDeleted::Start(e)), cx);
    }
    let deleting = cx.spawn(async move |cx: &mut AsyncApp| {
        let gone = match told.recv_async().await {
            Ok(gone) => gone.map_err(NotDeleted::from),
            Err(_) => Err(NotDeleted::Stopped),
        };
        cx.update(|cx| deleted(gone, cx));
    });
    dictating_mut(cx).deleting = Some(deleting);
}

/// The model's files are gone, or couldn't all be, which the person is told.
fn deleted(gone: Result<(), NotDeleted>, cx: &mut App) {
    dictating_mut(cx).deleting = None;
    reread_whats_on_disk(cx);
    if let Err(why) = gone {
        crate::asking::tell("Couldn't delete the model", format!("{why}."), cx);
    }
    follow_the_choice(cx);
}
