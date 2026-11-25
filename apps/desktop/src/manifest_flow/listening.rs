//! Listening for GitHub to send the person back: bound first, so the hub can be told where GitHub is to send them in the
//! manifest it writes, then serving the page that posts that manifest, at its path with the right `state`, until GitHub
//! sends the person back with a code, at its own, with the same `state`. Every other path and `state` is refused.

use std::io;
use std::time::Duration;

use protocol::client::github_apps::GithubManifestCode;
use ring::rand::{SecureRandom as _, SystemRandom};

use super::page;
use super::under::Under;
use crate::loopback::page::{Status, said};
use crate::loopback::{self, Answer, Asked};

protocol::tokens!(
    /// What the page sends GitHub with the manifest, and GitHub sends back with the code, which no one else knows, so
    /// the listener takes a code only from the flow it started, rather than one another page sent the browser with.
    ManifestState,
);

/// Where the page that posts the manifest is, which the browser opens on, with the `state` after it.
const MAKING: &str = "/make";
/// Where GitHub sends the person back, with the code and the `state` after it.
const MADE: &str = "/made";

/// A listener bound, on a port the system picked, not serving yet.
pub(crate) struct Bound {
    bound: loopback::Bound,
    state: ManifestState,
}

/// A listener serving the page, until it stops. Letting go of it stops it.
pub(crate) struct Listening {
    page_url: String,
    listening: loopback::Listening<GithubManifestCode>,
}

/// How a listener stopped by itself: with the code GitHub sent the person back with, or as its time limit passed.
pub(crate) type Ended = loopback::Ended<GithubManifestCode>;

impl Bound {
    /// Binds, with a `state` of its own.
    pub(crate) fn new() -> io::Result<Bound> {
        Ok(Bound { bound: loopback::Bound::new()?, state: new_state()? })
    }

    /// Where GitHub is to send the person back once they've made the app, for the hub's manifest to say.
    pub(crate) fn redirect_url(&self) -> String {
        format!("http://{}{MADE}", self.bound.address())
    }

    /// Serves the page that posts `manifest` to GitHub's form for making an app `under` the account or organization
    /// chosen, and takes GitHub's redirect back, until it has the code, it's let go of, or `limit` has passed.
    pub(crate) fn serve(self, manifest: &serde_json::Value, under: &Under, limit: Duration) -> io::Result<Listening> {
        let Bound { bound, state } = self;
        let page_url = format!("http://{}{MAKING}?state={}", bound.address(), state.as_str());
        let making = page::making(manifest, &under.form(), &state);
        let listening = bound.serve(move |asked| answer(asked, &state, &making), limit)?;
        Ok(Listening { page_url, listening })
    }
}

impl Listening {
    /// The page the browser is to open on, which posts the manifest to GitHub.
    pub(crate) fn page_url(&self) -> &str {
        &self.page_url
    }

    /// Where how it ended comes, for a task to wait on. Nothing comes once it's been let go of.
    pub(crate) fn ended(&self) -> flume::Receiver<Ended> {
        self.listening.ended()
    }
}

/// What `asked` is answered with, by the flow whose `state` it is, which serves the page `making`.
fn answer(asked: &Asked, state: &ManifestState, making: &str) -> Answer<GithubManifestCode> {
    let ours = || asked.query.value("state").as_deref() == Some(state.as_str());
    let not_ours = || Answer::Page(Status::BadRequest, said(page::NOT_OURS));
    match asked.path.as_str() {
        MAKING if ours() => Answer::Page(Status::Ok, making.to_string()),
        MADE if ours() => match asked.query.value("code") {
            Some(code) => Answer::Heard {
                heard: GithubManifestCode::new(code),
                taken: said(page::BACK),
                late: said(page::STOPPED),
            },
            None => not_ours(),
        },
        MAKING | MADE => not_ours(),
        _ => Answer::nothing_here(),
    }
}

/// The digits a state is written in.
const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 128 bits from the system's source of randomness, as hex.
fn new_state() -> io::Result<ManifestState> {
    let mut bytes = [0_u8; 16];
    SystemRandom::new().fill(&mut bytes).map_err(|_| io::Error::other("the system gave no randomness"))?;
    let hex = bytes.iter().flat_map(|byte| [DIGITS[usize::from(byte >> 4)], DIGITS[usize::from(byte & 0xf)]]);
    Ok(ManifestState::new(hex.map(char::from).collect::<String>()))
}
