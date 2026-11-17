//! The Git page's unit tests, but for what's drawn, which the gallery and the UI tests see.

/// Against a stand-in hub, which listens on a Unix socket.
#[cfg(unix)]
mod app_hub;
mod apps;
/// Against a stand-in hub, which listens on a Unix socket.
#[cfg(unix)]
mod hub;
mod kept;
mod made_up;
mod rows;
mod sign_ins;
#[cfg(unix)]
mod stand_in;
