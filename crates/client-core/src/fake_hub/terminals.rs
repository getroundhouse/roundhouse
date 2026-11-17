//! What a fake hub says of a terminal: what it shows, which never changes, at the size it has, which a view
//! can't change.

use protocol::client::terminals::TerminalAttach;
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubFrame};
use protocol::terminal::TerminalUpdate;

use super::serve::{Answer, Outcome};
use super::world::World;

/// What the world's terminal shows, as the frame that follows the reply.
pub(super) fn attach(world: &World, attach: &TerminalAttach) -> Answer {
    let (_, screen) = world
        .terminals
        .iter()
        .find(|(terminal, _)| terminal.id == attach.id)
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("a fake hub has no terminal {}", attach.id)))?;
    let update = TerminalUpdate::Screen(screen.clone());
    Ok(Outcome::Reply(ClientResponse::Ok, vec![HubFrame::Terminal { subscription_id: attach.subscription_id, update }]))
}
