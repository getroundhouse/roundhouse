//! Dictating into the composer, to a stand-in for the model, from audio the test gives it: live text under what's
//! typed, final text going in at the cursor, Esc putting the prompt back, nothing sent, and dictating ending once the
//! composer goes. Only a Mac hears.

#![cfg(test)]
#![cfg(target_os = "macos")]

mod another_app;
mod asking_first;
mod composer_going;
mod deleting;
mod fields;
mod hints;
mod holding_space;
mod key;
mod language;
mod proposals;
mod quieting;
mod support;
mod turning_on;
mod what_goes_in;

mod comments;
mod fine_line;
