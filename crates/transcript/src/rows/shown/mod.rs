//! What each row shows, for finding text in a chat: each text it draws with a match in it, as it draws it. It walks a
//! row as [`super::render_row`] draws it, with every block open, since finding opens what a match is in, and each
//! kind of call says what it shows beside where it draws it, in `tools`.
//!
//! What's counted is what's drawn: the person's and the agent's words, what calls were given and sent back, and what
//! went wrong, but not what the app writes itself, such as how many lines a read sent back, or a closed call's summary
//! of what it opens to. What asks the person something, a card or a question's line once it's answered, and the end of
//! the chat, with the text still streaming in and the prompts the agent hasn't read yet, aren't found. A test in the
//! desktop app's gallery draws every kind of call and checks that what each draws is what's counted here.

#[cfg(test)]
mod tests;

mod collecting;
#[cfg(test)]
pub(crate) use collecting::count;
pub(crate) use collecting::{Found, of_row, rows_drawing, rows_to_count};
pub(in crate::rows) use collecting::{Gather, of_block};
