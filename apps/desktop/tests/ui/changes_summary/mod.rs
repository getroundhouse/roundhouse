//! The sidebar of changes drawn from the hub's summary of the workspace's changed files, which a hub sends to a client
//! that follows the workspace, with a row for those it leaves out, which reads them a page at a time, and the file the
//! file tab shows read on its own. On a fake hub, whose world holds the summary, since what's drawn of it is the app's.

#![cfg(test)]

mod listed;
mod one_file;
mod one_path_twice;
mod world;
