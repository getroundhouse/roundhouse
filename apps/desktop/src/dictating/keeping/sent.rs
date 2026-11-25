//! What's sent from where a recording was kept, kept beside it, so what dictation wrote can be set against what the
//! person made of it. What's only typed is never kept: one thing sent is kept for each field and chat that something was
//! dictated into since the last thing sent from there, which outlasts the app.

use std::collections::HashSet;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use client_core::this_device::{NotKept, Saver, read_json};
use dictation_ui::Within;
use gpui_kit::App;
use kept_recordings::{SENT, SENT_VERSION, Sent, SentFrom, UNSENT};

use super::files::{gone_already, replace};
use super::kept::{Recordings, field, keeps, now_ms};

/// Where recordings were kept and nothing sent since.
pub(super) struct Unsent {
    from: HashSet<SentFrom>,
    saver: Saver<Save>,
}

/// What changed in the folder, written in the order it did.
struct Save {
    unsent: Vec<SentFrom>,
    /// A line of what was sent.
    sent: Option<Vec<u8>>,
}

impl Unsent {
    /// What waited in `folder` when the app last went.
    pub(super) fn read(folder: &Path) -> Self {
        let read = read_json(&folder.join(UNSENT)).unwrap_or_else(|error| {
            tracing::warn!(name: "dictation.unsent_unread", %error, "Couldn't read which recordings were unsent");
            None
        });
        let writing = folder.to_path_buf();
        let write = move |save: &Save| write(&writing, save);
        let failed = |error| tracing::warn!(name: "dictation.sent_not_kept", %error, "Couldn't keep what was sent");
        Self { from: read.unwrap_or_default(), saver: Saver::start(write, failed) }
    }

    /// A recording was kept from there, so the next thing sent from there is too.
    pub(super) fn recorded(&mut self, from: SentFrom) {
        if self.from.insert(from) {
            self.save(None);
        }
    }

    /// Keeps `text` if a recording waited for it, and gives how much room that takes.
    fn sent(&mut self, from: SentFrom, text: String) -> u64 {
        if !self.from.remove(&from) {
            return 0;
        }
        let sent = Sent { version: SENT_VERSION, sent_at_ms: now_ms(), from, text };
        // Text that was dictated and then deleted is a recording that nothing sent accounts for.
        let line = (!sent.text.trim().is_empty()).then(|| line(&sent)).transpose().unwrap_or_else(|error| {
            tracing::warn!(name: "dictation.sent_not_kept", %error, "Couldn't keep what was sent");
            None
        });
        let room_bytes = line.as_ref().map_or(0, Vec::len) as u64;
        self.save(line);
        room_bytes
    }

    /// Nothing sent from here on is for a recording kept before now.
    pub(super) fn forget(&mut self) {
        if !self.from.is_empty() {
            self.from.clear();
            self.save(None);
        }
    }

    /// Closes, or hears, once everything so far is written.
    pub(super) fn saved(&self) -> flume::Receiver<()> {
        self.saver.saved()
    }

    fn save(&self, sent: Option<Vec<u8>>) {
        self.saver.save(Save { unsent: self.from.iter().cloned().collect(), sent });
    }
}

/// Keeps what was sent, if a recording waits for it and the person still chooses to keep them.
pub(in crate::dictating) fn sent(sent: dictation_ui::Sent, cx: &mut App) {
    if !keeps(cx) {
        return;
    }
    let dictation_ui::Sent { place, within, text } = sent;
    let from = sent_from(place, within);
    let recordings = cx.global_mut::<Recordings>();
    recordings.room_bytes += recordings.unsent.sent(from, text);
}

pub(super) fn sent_from(place: dictation_ui::Place, within: Within) -> SentFrom {
    let Within { workspace, chat, said_out_loud } = within;
    SentFrom { field: field(place), workspace, chat, said_out_loud }
}

fn line(sent: &Sent) -> serde_json::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(sent)?;
    line.push(b'\n');
    Ok(line)
}

/// What waits is written first, so the app going in between loses what was sent rather than keeping the next, which may
/// only be typed.
fn write(folder: &Path, save: &Save) -> Result<(), NotKept> {
    let unsent = folder.join(UNSENT);
    write_unsent(folder, &unsent, &save.unsent).map_err(|error| NotKept::new(&unsent, error))?;
    let Some(line) = &save.sent else { return Ok(()) };
    let sent: PathBuf = folder.join(SENT);
    let open = private_files::file_options().create(true).append(true).open(&sent);
    open.and_then(|mut file| file.write_all(line)).map_err(|error| NotKept::new(&sent, error))
}

fn write_unsent(folder: &Path, file: &Path, unsent: &[SentFrom]) -> io::Result<()> {
    if unsent.is_empty() {
        return std::fs::remove_file(file).or_else(gone_already);
    }
    private_files::create_dir_all(folder)?;
    replace(file, &serde_json::to_vec(unsent)?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use kept_recordings::{Field, SENT, Sent, SentFrom, UNSENT};

    use super::Unsent;

    fn from(chat: &str) -> SentFrom {
        SentFrom {
            field: Field::Composer,
            workspace: Some("w1".to_string()),
            chat: Some(chat.to_string()),
            said_out_loud: false,
        }
    }

    fn saved(unsent: &Unsent) {
        unsent.saved().recv_timeout(Duration::from_secs(10)).unwrap();
    }

    fn texts(folder: &Path) -> Vec<String> {
        let lines = std::fs::read_to_string(folder.join(SENT)).unwrap_or_default();
        lines.lines().map(|line| serde_json::from_str::<Sent>(line).unwrap().text).collect()
    }

    #[test]
    fn what_waits_outlasts_the_app_and_what_was_sent_before_it_went_does_not_wait() {
        let data = tempfile::tempdir().unwrap();
        let folder = data.path().join("recordings");
        let mut before = Unsent::read(&folder);
        before.recorded(from("s2"));
        assert!(before.sent(from("s2"), "Sent before.".to_string()) > 0, "it takes room");
        before.recorded(from("s1"));
        saved(&before);
        drop(before);

        let mut after = Unsent::read(&folder);
        assert_eq!(after.sent(from("s2"), "Only typed.".to_string()), 0);
        after.sent(from("s1"), "Sent after.".to_string());
        after.sent(from("s1"), "Typed after that.".to_string());
        saved(&after);

        assert_eq!(texts(&folder), ["Sent before.", "Sent after."]);
        assert!(!folder.join(UNSENT).exists(), "nothing waits");
    }

    #[test]
    fn text_that_was_all_deleted_is_not_kept_and_no_longer_waits() {
        let data = tempfile::tempdir().unwrap();
        let mut unsent = Unsent::read(data.path());
        unsent.recorded(from("s1"));
        assert_eq!(unsent.sent(from("s1"), " ".to_string()), 0);
        unsent.sent(from("s1"), "Only typed.".to_string());
        saved(&unsent);
        assert!(texts(data.path()).is_empty());
    }

    #[test]
    fn what_cant_be_read_waits_for_nothing() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(UNSENT), "[{").unwrap();
        let mut unsent = Unsent::read(data.path());
        assert_eq!(unsent.sent(from("s1"), "Only typed.".to_string()), 0);
    }

    #[cfg(unix)]
    #[test]
    fn only_their_user_can_read_them() {
        use std::os::unix::fs::PermissionsExt as _;

        let data = tempfile::tempdir().unwrap();
        let mut unsent = Unsent::read(data.path());
        unsent.recorded(from("s1"));
        unsent.recorded(from("s2"));
        unsent.sent(from("s1"), "Fix it.".to_string());
        saved(&unsent);
        for file in [SENT, UNSENT] {
            let mode = std::fs::metadata(data.path().join(file)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{file}");
        }
    }
}
