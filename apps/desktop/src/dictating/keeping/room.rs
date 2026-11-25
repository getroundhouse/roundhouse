//! How much room the kept recordings take, as the box under Keep recordings in Settings says it, and deleting them.

use std::io;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{App, AppContext as _};

use super::kept::Recordings;
use crate::asking::Question;
use crate::status_box::{Action, StatusBox, Tone};

/// The box's, and its button's, for tests and a screen reader to find them by.
pub const STATUS: &str = "recordings-status";
pub const DELETE: &str = "recordings-delete";

/// How much room the recordings take, as a box, while there are any.
pub(crate) fn status_box(cx: &App) -> Option<StatusBox> {
    let recordings = cx.try_global::<Recordings>()?;
    let being_deleted = recordings.deleting.is_some();
    if recordings.room_bytes == 0 && !being_deleted {
        return None;
    }
    let delete = Action { id: DELETE, label: "Delete recordings", off: being_deleted, act: ask_to_delete };
    let (icon, tone, detail) = if being_deleted {
        (Lucide::LoaderCircle, Tone::Busy, "Deleting recordings.".to_string())
    } else {
        (Lucide::HardDrive, Tone::Quiet, format!("Recordings take {} on this Mac.", room(recordings.room_bytes)))
    };
    Some(StatusBox { id: STATUS, icon, tone, lead: None, detail: detail.into(), progress: None, action: Some(delete) })
}

/// `bytes` as a person reads them: in megabytes until there's a gigabyte.
fn room(bytes: u64) -> String {
    if bytes < 999_500_000 {
        return format!("{} MB", (bytes.max(1) + 500_000).max(1_000_000) / 1_000_000);
    }
    format!("{} GB", crate::dictating::gigabytes(bytes))
}

/// Deletes them once the person says so, since they can't be had back.
fn ask_to_delete(cx: &mut App) {
    let asked = Question {
        id: DELETE,
        title: "Delete recordings?".into(),
        says: "Every recording kept on this Mac is deleted, with what you sent from where you dictated, and can't be \
               had back."
            .into(),
        keep: "Cancel",
        act: "Delete",
        removes: true,
    };
    crate::asking::ask_in_front(asked, cx, |_, cx| delete(cx));
}

fn delete(cx: &mut App) {
    let recordings = cx.global_mut::<Recordings>();
    if recordings.deleting.is_some() {
        return;
    }
    recordings.drop_whats_being_written();
    recordings.unsent.forget();
    let saved = recordings.unsent.saved();
    let (folder, gone_from) = (recordings.folder.clone(), recordings.folder.clone());
    let deleting = cx.spawn(async move |cx| {
        // What was sent before now is written before it's deleted, rather than after.
        let _ = saved.recv_async().await;
        let gone = cx.background_spawn(async move { std::fs::remove_dir_all(&folder) }).await;
        let left_bytes = cx.background_spawn(async move { models::bytes_on_disk(&gone_from) }).await;
        cx.update(|cx| deleted(&gone, left_bytes, cx));
    });
    cx.global_mut::<Recordings>().deleting = Some(deleting);
}

/// They're gone, or couldn't all be, which the person is told.
fn deleted(gone: &io::Result<()>, left_bytes: u64, cx: &mut App) {
    let recordings = cx.global_mut::<Recordings>();
    recordings.deleting = None;
    // One recorded meanwhile may have been deleted too.
    recordings.unsent.forget();
    recordings.room_bytes = left_bytes;
    if let Err(error) = gone
        && error.kind() != io::ErrorKind::NotFound
    {
        crate::asking::tell("Couldn't delete the recordings", format!("{error}."), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::room;

    #[test]
    fn room_is_in_megabytes_until_theres_a_gigabyte() {
        assert_eq!(room(1), "1 MB");
        assert_eq!(room(1_600_000), "2 MB");
        assert_eq!(room(999_400_000), "999 MB");
        assert_eq!(room(999_500_000), "1.0 GB");
        assert_eq!(room(2_440_000_000), "2.4 GB");
    }
}
