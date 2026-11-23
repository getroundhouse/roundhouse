//! A stand-in hub that answers each watch of a terminal with what a test says it shows.

use std::path::Path;

use protocol::client::subscriptions::{Follow, Subscribed};
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::terminal::{TerminalExit, TerminalPicture, TerminalSize, TerminalUpdate};

use super::following::{SIZE, pictures_at};
use crate::stand_in_hub::{Does, listen, refusing_every_start, subscribed};

/// What a hub sends of a terminal it's watched: pictures, and maybe the shell's end.
pub(super) struct Watched {
    pub(super) pictures: Vec<TerminalPicture>,
    pub(super) exited: bool,
}

/// A hub that answers each watch of a terminal by sending what `watched_for` gives for how many watches there have
/// been, or that there's no such terminal for `None`, and says each other request it's sent, and each watch. It goes
/// away once it's sent what's typed, if `hangs_up_on_typing`.
pub(super) fn start_hub(
    socket: &Path,
    hangs_up_on_typing: bool,
    watched_for: impl Fn(usize) -> Option<Watched> + Send + 'static,
) -> flume::Receiver<ClientRequest> {
    let (asked, told) = flume::unbounded();
    let mut watched_count = 0;
    let ok = || Box::new(ReplyOutcome::Response(ClientResponse::Ok));
    listen(socket, move |request| {
        let Some(subscription_id) = watch_of(&request) else {
            let hangs_up = hangs_up_on_typing && matches!(request, ClientRequest::TerminalTyped(_));
            let answer = if let ClientRequest::Subscribe(subscribe) = &request {
                subscribed(subscribe, |_, _| Vec::new())
            } else {
                Does::Answers(ok())
            };
            let _ = asked.send(request);
            return if hangs_up { Does::HangsUp } else { answer };
        };
        watched_count += 1;
        let Some(watched) = watched_for(watched_count) else {
            let _ = asked.send(request.clone());
            return Does::Answers(Box::new(refusing_every_start(&request, ErrorCode::NotFound, "no such terminal")));
        };
        let mut sent: Vec<HubFrame> = watched
            .pictures
            .into_iter()
            .map(|picture| HubFrame::TerminalPicture { subscription_id, picture })
            .collect();
        if watched.exited {
            let update = TerminalUpdate::Exited(TerminalExit { code: Some(0) });
            sent.push(HubFrame::Terminal { subscription_id, update });
        }
        let started = ClientResponse::Subscribed(Subscribed::default());
        let _ = asked.send(request);
        Does::AnswersThen(Box::new(ReplyOutcome::Response(started)), sent)
    });
    told
}

/// The subscription ID a request to watch a terminal starts, if it's one: a `Subscribe` that starts following a
/// terminal.
pub(super) fn watch_of(request: &ClientRequest) -> Option<u64> {
    let ClientRequest::Subscribe(subscribe) = request else { return None };
    let watch = subscribe.start.iter().find(|start| matches!(start.follow, Follow::Terminal { .. }))?;
    Some(watch.subscription_id)
}

/// Whether `request` watches a terminal.
pub(super) fn watches(request: &ClientRequest) -> bool {
    watch_of(request).is_some()
}

/// A terminal that wrote each of `writes`, with a picture after each, and hasn't ended.
pub(super) fn showing(writes: &[&[u8]]) -> Watched {
    showing_at(SIZE, writes)
}

/// As [`showing`], of a terminal of `size`.
pub(super) fn showing_at(size: TerminalSize, writes: &[&[u8]]) -> Watched {
    Watched { pictures: pictures_at(size, writes), exited: false }
}
