//! Where the person is sent back to: at one path, with the page's `state`, which is this sign-in's. One with another
//! `state` is dropped, so whoever sent it ends nothing. The browser is told to go back to Roundhouse, and not that
//! they're signed in, since only the hub can say that, once it has had tokens for the code.

use std::io;
use std::time::Duration;

use protocol::client::tool_sign_ins::{REDIRECT_PATH, ToolSignInReturned};

use super::page::State;
use crate::loopback::page::{Status, said};
use crate::loopback::{Answer, Asked, Bound, Listening};

/// What the browser is told where the person was sent back with a code.
const APPROVED: &str = "Go back to Roundhouse, which is finishing the sign-in. You can close this page.";
/// What it's told where they were sent back with anything else, such as that they refused it.
const NOT_APPROVED: &str = "You aren't signed in. Go back to Roundhouse, which says why.";
/// What it's told where the sign-in had ended already.
const LATE: &str = "Roundhouse has stopped waiting for this sign-in. Go back to Roundhouse to sign in again.";
/// What it's told where the redirect isn't of the sign-in that's waiting.
const NOT_OURS: &str = "This isn't from the sign-in Roundhouse is waiting for. Go back to Roundhouse.";

/// Listens on `bound` for the person to be sent back with `state`, until it's let go of or `limit` has passed. What it
/// hears is the whole query, for the hub to read.
pub(super) fn listen(bound: Bound, state: State, limit: Duration) -> io::Result<Listening<ToolSignInReturned>> {
    bound.serve(move |asked| answer(asked, &state), limit)
}

fn answer(asked: &Asked, state: &State) -> Answer<ToolSignInReturned> {
    if asked.path != REDIRECT_PATH {
        return Answer::nothing_here();
    }
    if asked.query.value("state").as_deref() != Some(state.as_str()) {
        return Answer::Page(Status::BadRequest, said(NOT_OURS));
    }
    let approved = asked.query.value("code").is_some() && asked.query.value("error").is_none();
    Answer::Heard {
        heard: ToolSignInReturned::new(asked.query.as_str()),
        taken: said(if approved { APPROVED } else { NOT_APPROVED }),
        late: said(LATE),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;
    use crate::loopback::Ended;
    use crate::loopback::browsing::fetch;

    const NEVER: Duration = Duration::from_secs(600);
    const SOON: Duration = Duration::from_secs(5);

    fn listening(state: &str) -> (Listening<ToolSignInReturned>, SocketAddr) {
        let bound = Bound::new().unwrap();
        let address = bound.address();
        (listen(bound, State::new(state), NEVER).unwrap(), address)
    }

    #[test]
    fn it_takes_the_whole_query_sent_back_with_its_state_and_drops_any_other_redirect() {
        let (one, at) = listening("one");
        let (other, other_at) = listening("other");

        for target in
            ["/callback?code=c&state=other", "/callback?code=c", "/callback?state=one&state=other", "/callback"]
        {
            let dropped = fetch(&format!("http://{at}{target}"));
            assert_eq!((dropped.status, dropped.body), (400, said(NOT_OURS)), "{target}");
        }
        for target in ["/?code=c&state=one", "/callback/?code=c&state=one", "/Callback?code=c&state=one"] {
            assert_eq!(fetch(&format!("http://{at}{target}")).status, 404, "{target}");
        }
        assert!(one.ended().try_recv().is_err(), "none of those ended it");
        let back = fetch(&format!("http://{at}/callback?code=a%2Bb&state=one&iss=https%3A%2F%2Fauth.example.com"));

        assert_eq!((back.status, back.body), (200, said(APPROVED)));
        let heard = ToolSignInReturned::new("code=a%2Bb&state=one&iss=https%3A%2F%2Fauth.example.com");
        assert_eq!(one.ended().recv_timeout(SOON).unwrap(), Ended::Heard(heard), "as it came");
        let refused = fetch(&format!("http://{other_at}/callback?error=access_denied&state=other"));
        assert_eq!((refused.status, refused.body), (200, said(NOT_APPROVED)));
        assert!(matches!(other.ended().recv_timeout(SOON).unwrap(), Ended::Heard(_)), "each hears its own");
    }
}
