//! Every screen of the iPhone app, drawn from made-up data, with no real hub and no window on screen, and checked
//! against its approved image in the spec, as the desktop's gallery checks its own. Each screen gets a fake hub that
//! answers from fixed data, in `worlds.rs`, and the real app connects to it in a window the size of a phone's screen,
//! kept clear of a phone's status bar and home indicator, so what's drawn is what the app draws on a phone. A picture
//! shows the whole screen, since a phone shows one thing at a time. The design system's handheld components are drawn
//! too, each as a sheet, in `components.rs`. The images are in `docs/spec/screens/phone/`, and
//! what each screen looks like now goes in `.context/gallery/phone/`, for looking at.

#![cfg(test)]

mod approved;
mod capture;
mod components;
mod screens;
mod worlds;
