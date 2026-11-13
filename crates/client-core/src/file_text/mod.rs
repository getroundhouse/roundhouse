//! What a client holds of one file's text as a commit held it, read a page of lines at a time: how many lines it has,
//! and the pages it has of them, so a file too big to send whole can still be shown, as much of it as is looked at.
//!
//! No IO and no UI, as for a session's log: a view asks for lines by number, this says which pages to fetch, and
//! whoever owns the connection fetches them and hands them back, as `OpenFile` does. A page asked for is fetched once,
//! until it comes or fails, and one that didn't come is asked for again the next time it's needed. What it holds never
//! goes stale, since a commit, named by its full object ID, holds the same file for good.

#[cfg(test)]
mod laws;

mod pages;
pub use pages::FileText;
