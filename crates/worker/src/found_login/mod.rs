//! Who the machine's own `gh` is logged in to github.com as, and the token it uses, read as what the worker starts
//! under the machine's own finds it, for the hub to record for the worker's provider and to ask GitHub with.

mod finding;
/// Against a stand-in for `gh` that's a shell script.
#[cfg(test)]
#[cfg(unix)]
mod tests;
mod token;

#[cfg(test)]
#[cfg(unix)]
use finding::find;
pub(crate) use finding::reply;
