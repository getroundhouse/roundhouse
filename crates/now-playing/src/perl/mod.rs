//! The library `/usr/bin/perl` loads on a Mac: `MediaRemote`'s calls, and the one function perl calls, which pauses the
//! Now Playing app and holds the pause until the app lets go of it.

mod app;
mod block;
mod held;
mod media_remote;
