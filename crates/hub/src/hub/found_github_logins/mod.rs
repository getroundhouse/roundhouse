//! Who each provider's machines' own `gh` is logged in to github.com as, which the hub asks the first worker of each
//! provider it links to after it starts, and records, as `FindGithubLogin` says.

#[cfg(test)]
mod tests;

mod asked;
mod finding;
pub(super) use asked::ProvidersAsked;
#[cfg(test)]
use finding::FOUND_WITHIN;
