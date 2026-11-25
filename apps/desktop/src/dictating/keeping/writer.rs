//! Writing one recording, on a thread of its own, as it's dictated. Its file says all that's known from the start, and
//! its audio is written as it comes, so if the app goes before it ends, what there was of it can still be read.

use std::fs::File;
use std::io::{self, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dictation::SAMPLE_RATE;
use dictation_ui::{How, Said};
use flume::{Receiver, Selector};
use kept_recordings::{AUDIO, Device, Ended, FILE, Kept, Text};

/// What a recording's thread reads.
pub(super) struct Source {
    pub(super) audio: Receiver<Vec<f32>>,
    pub(super) said: Receiver<(Duration, Said)>,
    /// Set once the recording isn't to be kept after all.
    pub(super) dropped: Arc<AtomicBool>,
}

/// Writes the recording that `kept` says the start of into `folder`, from `source`, until it has ended and the last of
/// its audio has come. Gives how much room it takes, which is none if it was dropped.
pub(super) fn write(folder: &Path, kept: Kept, source: &Source) -> io::Result<u64> {
    let mut writer = Writer::create(folder, kept)?;
    let written = writer.follow(source);
    if source.dropped.load(Ordering::Acquire) {
        std::fs::remove_dir_all(&writer.dir).or_else(gone_already)?;
        return Ok(0);
    }
    written?;
    writer.finish()
}

struct Writer {
    dir: PathBuf,
    audio: File,
    data_bytes: u64,
    kept: Kept,
}

enum Next {
    Audio(Vec<f32>),
    AudioDone,
    Said(Duration, Said),
    SaidDone,
}

impl Writer {
    fn create(folder: &Path, kept: Kept) -> io::Result<Self> {
        let dir = folder.join(&kept.id);
        private_files::create_dir_all(&dir)?;
        let mut audio = private_files::file_options().create_new(true).write(true).open(dir.join(AUDIO))?;
        audio.write_all(&kept_recordings::header(SAMPLE_RATE, kept_recordings::MAX_DATA_BYTES))?;
        let writer = Self { dir, audio, data_bytes: 0, kept };
        writer.write_file()?;
        Ok(writer)
    }

    /// Writes what comes until there's no more of its audio or of what's said of it, in whichever order they end, or
    /// it's dropped.
    fn follow(&mut self, source: &Source) -> io::Result<()> {
        let (mut hearing, mut telling) = (true, true);
        while (hearing || telling) && !source.dropped.load(Ordering::Acquire) {
            let mut next = Selector::new();
            if hearing {
                next = next.recv(&source.audio, |audio| audio.map_or(Next::AudioDone, Next::Audio));
            }
            if telling {
                next = next.recv(&source.said, |said| said.map_or(Next::SaidDone, |(at, said)| Next::Said(at, said)));
            }
            match next.wait() {
                Next::Audio(samples) => self.audio(&samples)?,
                Next::AudioDone => hearing = false,
                Next::Said(at, said) => self.said(at, said),
                Next::SaidDone => telling = false,
            }
        }
        Ok(())
    }

    fn audio(&mut self, samples: &[f32]) -> io::Result<()> {
        let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        self.audio.write_all(&bytes)?;
        self.data_bytes += bytes.len() as u64;
        Ok(())
    }

    fn said(&mut self, at: Duration, said: Said) {
        let at_ms = u64::try_from(at.as_millis()).unwrap_or(u64::MAX);
        let mut text = |text| self.kept.said.push(kept_recordings::Said { at_ms, text });
        match said {
            Said::Opened(device) => {
                let dictation_ui::InputDevice { name, sample_rate, channels } = device;
                self.kept.device = Some(Device { name, sample_rate, channels });
            }
            Said::Live(live) => text(Text::Live(live)),
            Said::Final(sentence) => text(Text::Final(sentence)),
            Said::Stopped => self.kept.stopped_ms = Some(at_ms),
            Said::Ended { how, error } => self.kept.ended = Some(Ended { at_ms, how: kept_how(how), error }),
        }
    }

    /// Writes how much audio there is in its header, and the file again, with all that was said.
    fn finish(mut self) -> io::Result<u64> {
        let data_bytes = u32::try_from(self.data_bytes).unwrap_or(kept_recordings::MAX_DATA_BYTES);
        self.audio.seek(SeekFrom::Start(0))?;
        self.audio.write_all(&kept_recordings::header(SAMPLE_RATE, data_bytes))?;
        self.write_file()?;
        Ok(self.data_bytes + std::fs::metadata(self.dir.join(FILE))?.len())
    }

    /// Replaces its file all at once, so the app going as it's written leaves the last one whole.
    fn write_file(&self) -> io::Result<()> {
        let file = self.dir.join(FILE);
        let partial = file.with_extension("json.partial");
        let mut written = private_files::file_options().create(true).truncate(true).write(true).open(&partial)?;
        written.write_all(&serde_json::to_vec_pretty(&self.kept)?)?;
        std::fs::rename(&partial, &file)
    }
}

fn gone_already(error: io::Error) -> io::Result<()> {
    if error.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(error) }
}

fn kept_how(how: How) -> kept_recordings::How {
    match how {
        How::Heard => kept_recordings::How::Heard,
        How::NothingHeard => kept_recordings::How::NothingHeard,
        How::Cancelled => kept_recordings::How::Cancelled,
        How::FieldGone => kept_recordings::How::FieldGone,
        How::Failed => kept_recordings::How::Failed,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use dictation_ui::{How, Said};
    use kept_recordings::{AUDIO, FILE, Field, HEADER_BYTES, Kept, MAX_DATA_BYTES, VERSION};

    use super::{Source, write};

    fn starting() -> Kept {
        Kept {
            version: VERSION,
            id: "r1".to_string(),
            started_at_ms: 0,
            build: "dev".to_string(),
            model: "m1".to_string(),
            language: "auto".to_string(),
            hint_boost: 2.0,
            hints: Vec::new(),
            picked_from: None,
            field: Field::Composer,
            workspace: None,
            chat: None,
            said_out_loud: false,
            device: None,
            said: Vec::new(),
            stopped_ms: None,
            ended: None,
        }
    }

    /// What's read of the recording in `folder`: its file, how many bytes of audio its header says, and the samples.
    fn read(folder: &Path) -> (Kept, u32, Vec<f32>) {
        let kept = serde_json::from_slice(&std::fs::read(folder.join("r1").join(FILE)).unwrap()).unwrap();
        let audio = std::fs::read(folder.join("r1").join(AUDIO)).unwrap();
        let (header, data) = audio.split_at(HEADER_BYTES);
        let says = u32::from_le_bytes(header[40..].try_into().unwrap());
        (kept, says, data.as_chunks().0.iter().map(|sample| f32::from_le_bytes(*sample)).collect())
    }

    #[test]
    fn what_came_before_the_app_went_can_be_read() {
        let folder = tempfile::tempdir().unwrap();
        let (audio, taken) = flume::unbounded();
        let (tell, said) = flume::unbounded();
        let source = Source { audio: taken, said, dropped: Arc::new(AtomicBool::new(false)) };
        let dir = folder.path().to_path_buf();
        // Never ended, as when the app's killed: the thread is left waiting, and the files read as they stand.
        std::thread::spawn(move || write(&dir, starting(), &source));
        tell.send((Duration::from_millis(5), Said::Live("fix".to_string()))).unwrap();
        audio.send(vec![0.25, -0.5]).unwrap();
        waiting::thread::until(Duration::from_secs(10), "the audio".to_string(), || {
            std::fs::metadata(folder.path().join("r1").join(AUDIO))
                .is_ok_and(|file| file.len() == HEADER_BYTES as u64 + 8)
        });

        let (kept, says, samples) = read(folder.path());
        assert_eq!((kept, says), (starting(), MAX_DATA_BYTES), "its file says it never ended");
        assert_eq!(samples, [0.25, -0.5], "exactly what was heard");
    }

    #[test]
    fn audio_still_to_come_once_it_has_ended_is_written() {
        let folder = tempfile::tempdir().unwrap();
        let (audio, taken) = flume::unbounded();
        let (tell, said) = flume::unbounded();
        audio.send(vec![0.1]).unwrap();
        tell.send((Duration::from_millis(5), Said::Ended { how: How::Cancelled, error: None })).unwrap();
        let source = Source { audio: taken, said, dropped: Arc::new(AtomicBool::new(false)) };
        let dir = folder.path().to_path_buf();
        let writing = std::thread::spawn(move || write(&dir, starting(), &source).unwrap());
        drop(tell);
        // With nothing more to say of it, it goes on waiting for its audio.
        waiting::thread::for_what_mustnt_happen(Duration::from_millis(50));
        assert!(!writing.is_finished());
        audio.send(vec![0.2]).unwrap();
        drop(audio);

        assert!(writing.join().unwrap() > 8);
        let (kept, says, samples) = read(folder.path());
        assert_eq!((says, samples.as_slice()), (8, [0.1, 0.2].as_slice()));
        assert_eq!(kept.ended.unwrap().how, kept_recordings::How::Cancelled);
    }

    #[cfg(unix)]
    #[test]
    fn only_its_user_can_read_what_it_writes() {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = tempfile::tempdir().unwrap();
        let ((_, taken), (tell, said)) = (flume::unbounded(), flume::unbounded());
        tell.send((Duration::ZERO, Said::Ended { how: How::NothingHeard, error: None })).unwrap();
        drop(tell);
        let source = Source { audio: taken, said, dropped: Arc::new(AtomicBool::new(false)) };
        write(&folder.path().join("recordings"), starting(), &source).unwrap();

        let mode = |path: &str| {
            let at = folder.path().join(path);
            std::fs::metadata(at).unwrap().permissions().mode() & 0o777
        };
        assert_eq!((mode("recordings"), mode("recordings/r1")), (0o700, 0o700));
        assert_eq!((mode("recordings/r1/audio.wav"), mode("recordings/r1/recording.json")), (0o600, 0o600));
    }
}
