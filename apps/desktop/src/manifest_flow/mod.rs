//! The desktop's side of GitHub's manifest flow, which makes an app of your own in one click. GitHub takes an app's
//! manifest only as a form posted to it, not in a link, so the app serves, on `127.0.0.1` and a port the system picks,
//! a page that posts the manifest the hub wrote to GitHub's form for making an app, under the person's own account or
//! an organization's, with a `state` of its own. The browser opens on that page, and once the person has made the app,
//! GitHub sends them back to the app's address with a one-time code and the same `state`, which the app takes, and
//! gives the hub. It refuses any other path, and any other `state`, so nothing but the redirect from this flow is taken
//! for it. It listens only on loopback, so nothing on another machine can reach it, and it stops once it has the code,
//! once whatever started it lets go of it, and after a time limit. A paired device's app listens on its own machine,
//! where the browser is, which is right, since the hub takes only the code.

mod answering;
mod listening;
mod page;
mod under;

#[cfg(any(test, feature = "testing"))]
pub mod browsing;
#[cfg(test)]
mod tests;

pub(crate) use listening::{Bound, Heard};
pub(crate) use under::{NotAnOrganization, Organization, Under};
