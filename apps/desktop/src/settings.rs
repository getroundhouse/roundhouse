//! What someone has chosen in Settings, and Settings itself. A setting of the app's own belongs to this Mac rather than
//! to a hub, since the app can show several hubs, so they're kept in a file in the app's data directory and read once,
//! as the app opens. A setting a hub carries out is one of that hub's records, which Settings shows and changes.

use std::path::{Path, PathBuf};

use gpui_kit::{App, BorrowAppContext as _, Context, Global, PromptLevel, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod page;
#[cfg(test)]
mod tests;

pub(crate) use page::{KEY_CONTEXT, OfHub, page};

use crate::saving::{Failure, Saver, write_json};

/// Every setting that's the app's own, each with the value it has when nobody has chosen. A setting a hub carries
/// out is the hub's, which keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Settings {
    /// Thinking shows in full, rather than as a line to open.
    #[serde(default)]
    pub(crate) always_expand_thinking: bool,
    /// Settings this build doesn't know, which a newer one wrote, kept so that saving here doesn't lose them.
    #[serde(flatten)]
    others: Map<String, Value>,
}

/// The file the settings are kept in.
#[derive(Clone, Debug)]
struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    fn in_dir(dir: &Path) -> Self {
        Self { path: dir.join("settings.json") }
    }

    /// What's saved, or every setting as nobody has chosen when nothing is, or what is can't be read.
    fn read(&self) -> Settings {
        let Ok(text) = std::fs::read_to_string(&self.path) else { return Settings::default() };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("couldn't read the settings in {}, so they start as nobody has chosen: {e}", self.path.display());
            Settings::default()
        })
    }

    fn write(&self, settings: &Settings) -> Result<(), Failure> {
        write_json(&self.path, settings)
    }
}

/// The settings in force, and what saves them.
struct Current {
    settings: Settings,
    saver: Saver<Settings>,
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
    let (saver, failed) = Saver::start(move |settings| file.write(settings));
    cx.set_global(Current { settings, saver });
    cx.spawn(async move |cx| {
        while let Ok(failure) = failed.recv_async().await {
            let detail = format!("They're in force until the app quits. {failure}");
            cx.update(|cx| crate::alert::show(PromptLevel::Warning, "Couldn't save settings".into(), detail, cx));
        }
    })
    .detach();
    // Quitting gives this a moment, so a change made just before is saved.
    cx.on_app_quit(|cx| {
        let all_saved = cx.try_global::<Current>().map(|current| current.saver.saved());
        async move {
            if let Some(all_saved) = all_saved {
                let _ = all_saved.recv_async().await;
            }
        }
    })
    .detach();
}

/// The settings in force. Every setting is as nobody has chosen until they're loaded.
pub(crate) fn get(cx: &App) -> Settings {
    cx.try_global::<Current>().map(|current| current.settings.clone()).unwrap_or_default()
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

/// Has a view hear whenever the settings change.
pub(crate) fn observe<V: 'static>(
    cx: &mut Context<V>,
    changed: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> Subscription {
    cx.observe_global::<Current>(changed)
}
