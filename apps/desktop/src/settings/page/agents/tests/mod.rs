//! The Agents page's unit tests, but for what's drawn, which the gallery and the UI tests see.

mod card;
/// Against a stand-in hub, which listens on a Unix socket.
#[cfg(unix)]
mod hub;
mod made_up;
mod rows;
mod words;
