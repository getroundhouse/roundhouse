//! Puts the app's icon in `roundhouse-desktop.exe`, as icon 1. GPUI's Windows backend loads icon 1 from its own program
//! once, as its platform starts, and registers it on the one window class its windows are made from. Windows shows a
//! class's icon on each of its windows and on their taskbar button.
//!
//! The icon is the `.ico` that `tools/package/ico.py` makes from the app's drawings, which the installer has too, so
//! this knows nothing of the drawings. It turns the `.ico` into a compiled resource file, a `.res`, as `rc.exe` would,
//! and has the linker put that in the program. MSVC's linker and LLVM's both take a `.res` as it is, so building needs
//! no resource compiler, on Windows or from a Mac.

mod res;

#[cfg(test)]
use anyhow::Context as _;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::Path;

mod embedding;
pub(crate) use embedding::embed;
