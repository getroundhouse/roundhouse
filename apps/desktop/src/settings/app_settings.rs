use crate::saving::saved_before_quitting;
use crate::settings::Choice;
use crate::sounds::Sound;
use client_core::this_device::{NotKept, Saver, read_json, write_json};
use client_core::workspace_order::Sort;
use gpui_kit::{App, BorrowAppContext as _, Context, Global, Subscription};
use protocol::model::RepoId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sound::Quieting;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every setting that's the app's own, each with the value it has when nobody has chosen. A setting a hub carries
/// out is the hub's, which keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a switch of its own in Settings, independent of the others, and saved as true or false"
)]
pub(crate) struct Settings {
    #[serde(default)]
    pub(crate) hubs: Vec<crate::app::saved_hubs::SavedHub>,
    #[serde(default)]
    pub(crate) selected_hub: Option<protocol::pairing::Fingerprint>,
    /// Thinking shows in full, rather than as a line to open.
    #[serde(default)]
    pub(crate) always_expand_thinking: bool,
    /// The order of the sidebar's workspaces, in each project, last active first unless another is chosen. Chosen
    /// from the sidebar's heading rather than in Settings, and kept here so it lasts.
    #[serde(default)]
    pub(crate) sidebar_sort: Sort,
    /// The projects the sidebar shows collapsed, with their workspaces put away. Chosen from each project's row, and
    /// kept here so it lasts. Nothing takes out a project that's no longer listed, since the app can show several
    /// hubs, and it may be another hub's.
    #[serde(default)]
    pub(crate) collapsed_projects: BTreeSet<RepoId>,
    /// The order of the sidebar's projects, from the top, once one has been dragged. Until then, and for any project
    /// this doesn't name, it's the hub's. Kept here so it lasts, as which are collapsed is.
    #[serde(default)]
    pub(crate) project_order: Vec<RepoId>,
    /// A notification tells of each chat that comes to need the person, unless it's the one being read. On unless
    /// turned off, since an agent finishing while you're elsewhere is what the app is for.
    #[serde(default = "on")]
    pub(crate) notify: bool,
    /// `sound` plays as a chat comes to need the person, whether or not a notification tells of it.
    #[serde(default = "on")]
    pub(crate) play_sound: bool,
    #[serde(default, deserialize_with = "Sound::or_chime")]
    pub(crate) sound: Sound,
    /// Whether prompts and written answers can be dictated to, once the model is downloaded and prepared. It asks first until the
    /// person chooses, since turning it on downloads 2.4 GB.
    #[serde(default)]
    pub(crate) dictation: Choice,
    /// The language what's dictated is heard in. Auto unless chosen, since the model hears any of its languages under
    /// Auto, and a wrong one far worse.
    #[serde(default, with = "crate::dictating::language")]
    pub(crate) dictation_language: dictation::Language,
    /// Whether the sound output is muted while something is dictated, so what's playing doesn't drown out what's said
    /// or get heard with it. Off unless chosen, since it changes the whole system's sound rather than the app's.
    #[serde(default)]
    pub(crate) mute_while_dictating: bool,
    /// Whether what the Now Playing app plays is paused while something is dictated, and played again after. Off
    /// unless chosen, since it pauses another app.
    #[serde(default)]
    pub(crate) pause_while_dictating: bool,
    /// Whether what's dictated is kept on this Mac, with its audio. Off unless chosen, since it's what someone said.
    #[serde(default)]
    pub(crate) keep_recordings: bool,
    /// Whether the agent's replies can be read out, in conversation mode, once the voice is downloaded. It asks first
    /// until the person chooses, as dictation does, since turning it on downloads the voice.
    #[serde(default)]
    pub(crate) speech: Choice,
    /// How fast replies are read out. Normal unless chosen.
    #[serde(default, with = "crate::settings::speech_speed")]
    pub(crate) speech_speed: conversation::Speed,
    /// Settings this build doesn't know, which a newer one wrote, kept so that saving here doesn't lose them.
    #[serde(flatten)]
    pub(super) others: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hubs: Vec::new(),
            selected_hub: None,
            always_expand_thinking: false,
            sidebar_sort: Sort::default(),
            collapsed_projects: BTreeSet::new(),
            project_order: Vec::new(),
            notify: on(),
            play_sound: on(),
            sound: Sound::default(),
            dictation: Choice::default(),
            dictation_language: dictation::Language::default(),
            mute_while_dictating: false,
            pause_while_dictating: false,
            keep_recordings: false,
            speech: Choice::default(),
            speech_speed: conversation::Speed::default(),
            others: Map::new(),
        }
    }
}

impl Settings {
    /// Whether what's dictated quiets what's playing `how` says, as the person has chosen.
    pub(crate) fn quiets_while_dictating(&self, how: Quieting) -> bool {
        match how {
            Quieting::MuteOutput => self.mute_while_dictating,
            Quieting::PauseMedia => self.pause_while_dictating,
        }
    }

    pub(crate) fn set_quiets_while_dictating(&mut self, how: Quieting, on: bool) {
        match how {
            Quieting::MuteOutput => self.mute_while_dictating = on,
            Quieting::PauseMedia => self.pause_while_dictating = on,
        }
    }
}

/// What a setting that's on unless turned off is when nobody has chosen.
fn on() -> bool {
    true
}

/// The file the settings are kept in.
#[derive(Clone, Debug)]
pub(super) struct SettingsFile {
    pub(super) path: PathBuf,
}

impl SettingsFile {
    pub(super) fn in_dir(dir: &Path) -> Self {
        Self { path: dir.join("settings.json") }
    }

    /// What's saved, or every setting as nobody has chosen when nothing is, or what is can't be read.
    pub(super) fn read(&self) -> Settings {
        let read = read_json(&self.path).unwrap_or_else(|error| {
            tracing::warn!(
                name: "settings.unread",
                %error,
                "Couldn't read the settings, so they start as nobody has chosen"
            );
            None
        });
        read.unwrap_or_default()
    }

    pub(super) fn write(&self, settings: &Settings) -> Result<(), NotKept> {
        write_json(&self.path, settings)
    }
}

/// The settings in force, and what saves them.
struct Current {
    pub(super) settings: Settings,
    pub(super) saver: Saver<Settings>,
}

impl Global for Current {}

/// Reads the settings saved in `data_dir`, once, before anything shows them, and has the app wait for them to be
/// saved as it quits.
pub(crate) fn load(data_dir: &Path, cx: &mut App) {
    if cx.has_global::<Current>() {
        return;
    }
    let file = SettingsFile::in_dir(data_dir);
    let settings = file.read();
    let (failures, failed) = flume::unbounded();
    let saver = Saver::start(move |settings| file.write(settings), move |failure| drop(failures.send(failure)));
    cx.set_global(Current { settings, saver });
    cx.spawn(async move |cx| {
        while let Ok(failure) = failed.recv_async().await {
            let detail = format!("They're in force until the app quits. {failure}");
            cx.update(|cx| crate::asking::tell("Couldn't save settings", detail, cx));
        }
    })
    .detach();
    saved_before_quitting(|current: &Current| current.saver.saved(), cx);
}

/// The settings in force. Every setting is as nobody has chosen until they're loaded.
pub(crate) fn get(cx: &App) -> Settings {
    cx.try_global::<Current>().map_or_default(|current| current.settings.clone())
}

/// Changes the settings, and saves them in the background. Whatever shows them draws again.
pub(crate) fn change(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let Some(current) = cx.try_global::<Current>() else { return };
    let mut settings = current.settings.clone();
    change(&mut settings);
    if settings == current.settings {
        return;
    }
    current.saver.save(settings.clone());
    cx.update_global::<Current, _>(|current, _| current.settings = settings);
}

/// Has the app hear whenever the settings change, for what follows a setting outside any view.
pub(crate) fn observe_in_app(cx: &mut App, changed: impl FnMut(&mut App) + 'static) -> Subscription {
    cx.observe_global::<Current>(changed)
}

/// Has a view hear whenever the settings change.
pub(crate) fn observe<V: 'static>(
    cx: &mut Context<V>,
    changed: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> Subscription {
    cx.observe_global::<Current>(changed)
}
