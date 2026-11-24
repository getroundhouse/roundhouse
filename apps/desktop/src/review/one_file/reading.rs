//! Reading the file the file tab shows when the summary moves it, and taking in what the read found.

use gpui_kit::{AppContext as _, Context};
use protocol::model::FileDiff;

use super::file::{Key, OneFile, Read, Taken};
use super::pages::{Unread, every_page, taken};
use crate::review::{ChangedEntry, Review, ReviewEvent};

impl Review {
    /// Says which file the file tab shows the diff of, or none, and reads it.
    pub fn show_diff_of(&mut self, entry: Option<ChangedEntry>, cx: &mut Context<Self>) {
        self.diffed = entry;
        self.read_the_file_again_if_it_moved(cx);
    }

    /// Reads the file the file tab shows again if the summary says something else of it than when it was last read,
    /// its last read failed, or it's another file, and lets go of the one read once the tab shows none. The summary
    /// can be current or as of an earlier read: reading the file starts the workspace's worker, after which the summary
    /// turns current. Nothing is read of a file the summary marks as named in bytes that aren't UTF-8.
    pub(in crate::review) fn read_the_file_again_if_it_moved(&mut self, cx: &mut Context<Self>) {
        let Some(listed) = &mut self.listed else { return };
        let Some(entry) = self.diffed.clone() else {
            listed.file = None;
            return;
        };
        let key = self.key_of(&entry);
        let Some(listed) = &self.listed else { return };
        let previous = match listed.file.as_ref().filter(|file| file.of(&entry)) {
            Some(OneFile { key: read_as, read: Read::Taken(_) | Read::Reading { .. }, .. }) if *read_as == key => {
                return;
            }
            Some(file) => file.shown().cloned(),
            None => None,
        };
        self.read_one_file(previous, cx);
    }

    /// Reads the file the file tab shows with one file's diff, every page of it, showing `previous` until it's read,
    /// unless the summary marks it as named in bytes that aren't UTF-8, so the name it gives names no file to read.
    /// Nothing, while the worktree can't be read.
    pub(in crate::review) fn read_one_file(&mut self, previous: Option<Taken>, cx: &mut Context<Self>) {
        let read = |entry: &ChangedEntry| self.worktree_readable && !self.names_no_file(entry);
        let Some(entry) = self.diffed.clone().filter(read) else { return };
        let key = self.key_of(&entry);
        let at = key.as_ref().map_or_else(|| entry.at(), |key| key.at.clone());
        let from = key.as_ref().and_then(|key| key.from.clone());
        let listed = key.as_ref().and_then(|key| key.version.clone());
        let Some(summary) = &mut self.listed else { return };
        summary.file = Some(OneFile { entry: entry.clone(), key, read: Read::Reading { previous } });
        self.one_file_asks += 1;
        let (asked, client, id) = (self.one_file_asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let read =
                cx.background_spawn(async move { every_page(&client, &id, &at, from.as_deref(), listed.as_ref()) });
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
        let Some(file) = self.listed.as_mut().and_then(|listed| listed.file.as_mut()) else { return };
        let status = file.key.as_ref().map(|key| key.status);
        let from = file.key.as_ref().and_then(|key| key.from.clone());
        let previous = file.shown().cloned();
        match read.and_then(|diff| taken(&file.entry.path, from, status, diff)) {
            Ok(taken) => file.read = Read::Taken(taken),
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

    /// What the summary says of `entry`.
    fn key_of(&self, entry: &ChangedEntry) -> Option<Key> {
        let summarized = self.listed.as_ref()?.summarized.as_ref()?;
        let file = entry.in_summary(summarized)?;
        Some(Key {
            at: file.at(),
            base: summarized.base_commit.clone(),
            status: file.status,
            from: file.from.clone(),
            version: file.version.clone(),
        })
    }
}
