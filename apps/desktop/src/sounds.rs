//! The sound for a chat that needs a look: the app's own, Chime, or one of this computer's alert sounds, by name. The
//! app plays it itself rather than through a notification, so it plays with notifications turned off too.

use gpui_kit::{App, AppContext as _, Global};
use serde::{Deserialize, Deserializer, Serialize};

/// Chime, which `tools/sounds/chime.py` makes. It's the project's own, synthesized there.
#[cfg(target_os = "macos")]
const CHIME: &[u8] = include_bytes!("../sounds/chime.wav");

/// A sound to play, as the settings keep it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Sound {
    /// The app's own.
    #[default]
    Chime,
    /// One of the computer's alert sounds, or one the person added, by its file's name without the extension.
    Mac(MacSound),
}

impl Sound {
    /// What Settings calls it.
    pub(crate) fn name(&self) -> &str {
        match self {
            Sound::Chime => "Chime",
            Sound::Mac(sound) => &sound.0,
        }
    }

    /// The sound saved, or Chime for one this build can't read, such as one a newer build added, so the rest of the
    /// settings still read.
    pub(crate) fn or_chime<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = serde_json::Value::deserialize(deserializer)?;
        Ok(serde_json::from_value(saved).unwrap_or_default())
    }
}

/// An alert sound's name, as macOS finds it in the system's, the computer's and the person's `Library/Sounds`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct MacSound(String);

/// Every sound there is to choose, Chime first and then this computer's by name, each once. It reads the folders
/// they're in, so it runs off the main thread, as [`read_list`] has it.
fn listed() -> Vec<Sound> {
    let mut theirs = computers();
    theirs.sort_by_key(|name| name.to_lowercase());
    theirs.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    std::iter::once(Sound::Chime).chain(theirs.into_iter().map(|name| Sound::Mac(MacSound(name)))).collect()
}

/// The sounds there are to choose, as last read.
struct Listed(Vec<Sound>);

impl Global for Listed {}

/// Reads again which sounds there are to choose, off the main thread, as Settings opens, so one added since shows.
pub(crate) fn read_list(cx: &mut App) {
    let reading = cx.background_spawn(async { listed() });
    cx.spawn(async move |cx| {
        let list = reading.await;
        cx.update(|cx| cx.set_global(Listed(list)));
    })
    .detach();
}

/// The sounds to choose from, as last read, with `chosen` among them even if it wasn't there to read. Only Chime and
/// `chosen` before the first read.
pub(crate) fn choices(chosen: &Sound, cx: &App) -> Vec<Sound> {
    let mut choices = cx.try_global::<Listed>().map_or_else(|| vec![Sound::Chime], |listed| listed.0.clone());
    if !choices.contains(chosen) {
        choices.push(chosen.clone());
    }
    choices
}

/// What plays sounds. Only the app as it runs has one, so a test or a picture of the app plays nothing.
pub(crate) struct Speaker;

impl Global for Speaker {}

/// Plays the sound at once, stopping whatever this played before. One of the computer's that has gone since it was
/// chosen plays Chime instead.
pub(crate) fn play(sound: &Sound, cx: &mut App) {
    #[cfg(feature = "testing")]
    cx.default_global::<Played>().0.push(sound.name().to_string());
    if cx.has_global::<Speaker>() {
        speak(sound);
    }
}

#[cfg(target_os = "macos")]
fn speak(sound: &Sound) {
    mac::play(sound);
}

/// Only a Mac plays one yet.
#[cfg(not(target_os = "macos"))]
fn speak(_sound: &Sound) {}

/// The sounds played, by name, oldest first, for a test to read.
#[cfg(feature = "testing")]
#[derive(Default)]
struct Played(Vec<String>);

#[cfg(feature = "testing")]
impl Global for Played {}

/// The sounds played so far, by name, oldest first, whether or not there was a speaker to play them.
#[cfg(feature = "testing")]
pub fn played(cx: &App) -> Vec<String> {
    cx.try_global::<Played>().map_or_default(|played| played.0.clone())
}

/// The names of this computer's alert sounds. Only a Mac has them, in each `Library/Sounds`.
fn computers() -> Vec<String> {
    #[cfg(target_os = "macos")]
    return mac::sounds();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use objc2::AnyThread as _;
    use objc2::rc::Retained;
    use objc2_app_kit::NSSound;
    use objc2_foundation::{
        NSData, NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains, NSString,
    };

    use super::{CHIME, Sound};

    /// The kinds of file `AppKit` plays that alert sounds come as.
    const KINDS: [&str; 7] = ["aiff", "aif", "aifc", "caf", "wav", "mp3", "m4a"];

    thread_local! {
        /// The sound playing, kept until the next, since `AppKit` stops one that's let go of.
        static PLAYING: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }

    pub(super) fn play(sound: &Sound) {
        let theirs = match sound {
            Sound::Chime => None,
            Sound::Mac(name) => NSSound::soundNamed(&NSString::from_str(&name.0)),
        };
        let Some(sound) = theirs.or_else(|| NSSound::initWithData(NSSound::alloc(), &NSData::with_bytes(CHIME))) else {
            return;
        };
        PLAYING.with_borrow_mut(|playing| {
            if let Some(was) = playing.take() {
                was.stop();
            }
            sound.play();
            *playing = Some(sound);
        });
    }

    /// The sounds in each `Library/Sounds`, the person's, the computer's and the system's, by name.
    pub(super) fn sounds() -> Vec<String> {
        let libraries = NSSearchPathForDirectoriesInDomains(
            NSSearchPathDirectory::LibraryDirectory,
            NSSearchPathDomainMask::AllDomainsMask,
            true,
        );
        let folders = libraries.iter().map(|library| PathBuf::from(library.to_string()).join("Sounds"));
        folders
            .flat_map(|folder| std::fs::read_dir(folder).into_iter().flatten().flatten())
            .filter_map(|entry| {
                let path = entry.path();
                let kind = path.extension()?.to_str()?.to_lowercase();
                KINDS.contains(&kind.as_str()).then(|| path.file_stem()?.to_str().map(str::to_string)).flatten()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{MacSound, Sound};

    #[test]
    fn a_sound_reads_back_as_it_was_saved() {
        for sound in [Sound::Chime, Sound::Mac(MacSound("Glass".to_string()))] {
            let saved = serde_json::to_value(&sound).unwrap();

            assert_eq!(Sound::or_chime(saved).unwrap(), sound);
        }
    }

    #[test]
    fn a_sound_this_build_cant_read_is_chime() {
        for saved in [serde_json::json!("whistle"), serde_json::json!({"bundled": "whistle"}), serde_json::json!(3)] {
            assert_eq!(Sound::or_chime(saved).unwrap(), Sound::Chime);
        }
    }

    #[test]
    fn chime_comes_first_and_every_other_once() {
        let listed = super::listed();

        assert_eq!(listed.first(), Some(&Sound::Chime));
        let names: Vec<String> = listed.iter().map(|sound| sound.name().to_lowercase()).collect();
        let mut once = names.clone();
        once.dedup();
        assert_eq!(names, once);
    }
}
