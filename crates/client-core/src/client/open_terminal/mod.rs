//! A terminal someone has open: the client's copy of what it shows, kept from what the hub sends of it on the thread
//! it's followed on, which starts again from what the terminal shows once the subscription falls behind, or there's a
//! new connection. A view takes a plain copy of what it shows, and gives the copy what it's given, such as keys, which
//! the copy turns into what the program reads, by the modes it set, and sends on.

mod copy;
mod following;
#[cfg(test)]
mod tests;

pub use following::OpenTerminal;
