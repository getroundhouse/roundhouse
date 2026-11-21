//! The thread that reads out: it loads the voice and opens the speakers when it's first given something to say, makes
//! each phrase the [`crate::reader::Reader`] gives it and plays it, passes on how far the speakers have got, and closes
//! them once nothing has played for a while. What to read, and what each mark means, the reader decides.

pub(crate) mod parts;
mod thread;

pub use thread::Speaking;
