//! Reading the file the file tab shows when the summary moves it, and taking in what the read found.

use gpui_kit::{AppContext as _, Context, SharedString};
use protocol::model::FileDiff;

use super::file::{Key, OneFile, Read, Taken};
use super::pages::{Unread, every_page, taken};
use crate::review::listed::{Listed, Summary, SummaryFile};
use crate::review::{Review, ReviewEvent};

impl Review {
    /// Says which file the file tab shows the diff of, or none, and reads it, while the summary is current.
    pub fn show_diff_of(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.diffed = path;
        self.read_the_file_again_if_it_moved(cx);
    }

    /// Reads the file the file tab shows again if the summary says something else of it than when it was last read,
    /// its last read failed, or it's another file, and lets go of the one read once the tab shows none. Once the
    /// summary listed isn't current, the file read with it shows as it was read, and another file is read with the
    /// whole diff, which then takes over, as it would at the next moment that reads it.
    pub(in crate::review) fn read_the_file_again_if_it_moved(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.diffed.clone() else {
            if let Some(Listed::Summary(Summary { file: SummaryFile::OneFile(file), .. })) = &mut self.listed {
                *file = None;
            }
            return;
        };
        let key = self.key_of(&path);
        // From the whole diff, at the moments that's read, where it's listed or the hub refused one file's diff.
        let Some(Listed::Summary(Summary { current, file: SummaryFile::OneFile(file), .. })) = &self.listed else {
            return;
        };
        let read_for = file.as_ref().filter(|file| file.of(&path));
        if !*current {
            if read_for.is_none() {
                self.refresh(cx);
            }
            return;
        }
        let previous = match read_for {
            Some(OneFile { key: read_as, read: Read::Taken(_) | Read::Reading { .. }, .. }) if *read_as == key => {
                return;
            }
            Some(file) => file.shown().cloned(),
            None => None,
        };
        self.read_one_file(previous, cx);
    }

    /// Reads the file the file tab shows with one file's diff, every page of it, showing `previous` until it's read.
    pub(in crate::review) fn read_one_file(&mut self, previous: Option<Taken>, cx: &mut Context<Self>) {
        let Some(path) = self.diffed.clone() else { return };
        let key = self.key_of(&path);
        let Some(Listed::Summary(Summary { file: SummaryFile::OneFile(file), .. })) = &mut self.listed else { return };
        let from = key.as_ref().and_then(|key| key.from.clone());
        let listed = key.as_ref().and_then(|key| key.version.clone());
        *file = Some(OneFile { path: path.clone(), key, read: Read::Reading { previous } });
        self.one_file_asks += 1;
        let (asked, client, id) = (self.one_file_asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let read =
                cx.background_spawn(async move { every_page(&client, &id, &path, from.as_deref(), listed.as_ref()) });
            let read = read.await;
            let _ = this.update(cx, |t, cx| t.took_one_file(asked, read, cx));
        })
        .detach();
    }

    /// Takes in what reading the file the tab shows found, unless another read has been asked for since.
    fn took_one_file(&mut self, asked: u64, read: Result<FileDiff, Unread>, cx: &mut Context<Self>) {
        if asked != self.one_file_asks {
            return;
        }
        let Some(Listed::Summary(summary)) = &mut self.listed else { return };
        let SummaryFile::OneFile(Some(file)) = &mut summary.file else { return };
        let status = file.key.as_ref().map(|key| key.status);
        let from = file.key.as_ref().and_then(|key| key.from.clone());
        let previous = file.shown().cloned();
        match read.and_then(|diff| taken(&file.path, from, status, diff)) {
            Ok(taken) => file.read = Read::Taken(taken),
            // A hub, or a worker, from before it, so the file is read with the whole diff, as from a hub that keeps
            // no summary.
            Err(Unread::Hub(e)) if e.is_unsupported() => {
                summary.file = SummaryFile::WholeDiff(None);
                self.refresh(cx);
                return;
            }
            Err(unread) => {
                file.read = Read::Failed { previous };
                if let Some(message) = unread.said() {
                    cx.emit(ReviewEvent::Error(message));
                }
            }
        }
        cx.emit(ReviewEvent::Loaded);
        cx.notify();
    }

    /// What the summary says of the file at `path`.
    fn key_of(&self, path: &str) -> Option<Key> {
        let Some(Listed::Summary(Summary { summarized, .. })) = &self.listed else { return None };
        let file = summarized.file(path)?;
        Some(Key {
            base: summarized.base_commit.clone(),
            status: file.status,
            from: file.from.clone(),
            version: file.version.clone(),
        })
    }
}
