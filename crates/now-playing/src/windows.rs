//! Windows' media sessions, through its public API for them, which pauses the current session, the one the keyboard's
//! media keys control, as a Mac pauses its Now Playing app. Each call waits on the system's media service. A `join`
//! waits for the service's answer with no limit, so a service that never answers holds up the thread `dictation`
//! pauses on, and so only the pauses after it, and the app's quitting only as long as quitting waits for anything.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::core::HSTRING;

use crate::now_playing::{ASKED_EVERY, NowPlaying, PAUSE_SHOWS_WITHIN, wait_until_paused};

/// Whether this platform can pause what's playing while it's dictated to. Windows can, through its public API.
pub const PAUSES: bool = true;

/// The session [`pause`] paused, by its app, and the manager to ask again which session is current.
pub struct Paused {
    manager: Manager,
    app: HSTRING,
}

/// Pauses the current session, if it's playing, waits until the pause shows, and says which it was. `None` where
/// nothing was paused, which is also where the media service didn't answer, or the session's app refused.
pub fn pause() -> Option<Paused> {
    let manager = Manager::RequestAsync().ok()?.join().ok()?;
    let (session, now) = current(&manager)?;
    let app = now.app_to_pause()?;
    if !session.TryPauseAsync().ok()?.join().ok()? {
        return None;
    }
    let now = || current(&manager).map(|(_, now)| now);
    wait_until_paused(&app, now, ASKED_EVERY, PAUSE_SHOWS_WITHIN);
    Some(Paused { manager, app })
}

/// Plays again what [`pause`] paused, as [`NowPlaying::plays_again`] decides.
pub fn play_again(Paused { manager, app }: Paused) {
    let Some((session, now)) = current(&manager) else { return };
    if now.plays_again(&app) {
        // Nothing more can be done where it refuses.
        let _ = session.TryPlayAsync().and_then(|played| played.join());
    }
}

/// The current session, and what it says of itself. `None` where there's none, or it can't be read.
fn current(manager: &Manager) -> Option<(Session, NowPlaying<HSTRING>)> {
    let session = manager.GetCurrentSession().ok()?;
    let status = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()?;
    let app = session.SourceAppUserModelId().ok()?;
    Some((session, NowPlaying { app: Some(app), playing: status == Status::Playing }))
}
