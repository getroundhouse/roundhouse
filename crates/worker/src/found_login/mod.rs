//! Who the machine's own `gh` is logged in to github.com as, and whether GitHub takes its token, read as what the
//! worker starts under the machine's own finds it, for the hub to record for the worker's provider.

mod checking;
mod finding;
/// Against a stand-in for `gh` that's a shell script.
#[cfg(test)]
#[cfg(unix)]
mod tests;

#[cfg(test)]
#[cfg(unix)]
use finding::find;
pub(crate) use finding::reply;
