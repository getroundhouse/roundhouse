//! What a fake hub says of a terminal: what it shows, which never changes, at the size it has, which a view
//! can't change. It's made from the bytes the world gives, the same way on every call, so a picture's numbering and
//! the rows asked for in it agree.

use protocol::client::terminals::{TerminalAttach, TerminalBounds, TerminalRows, TerminalText, TerminalWatch};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubFrame};
use protocol::model::TerminalId;
use protocol::terminal::{PictureSending, TerminalScreen, TerminalUpdate};
use terminal_state::{Mirror, MirrorError};

use super::serve::{Answer, Outcome};
use super::world::World;

/// What the world's terminal shows, as the frame that follows the reply.
pub(super) fn attach(world: &World, attach: &TerminalAttach) -> Answer {
    let update = TerminalUpdate::Screen(screen(world, &attach.id)?.clone());
    Ok(Outcome::Reply(ClientResponse::Ok, vec![HubFrame::Terminal { subscription_id: attach.subscription_id, update }]))
}

/// A whole picture of what the world's terminal shows, in as many frames as it takes, after the reply.
pub(super) fn watch(world: &World, watch: &TerminalWatch) -> Answer {
    let mut mirror = mirror(world, &watch.id)?;
    let shown = mirror.shown().map_err(|e| failed(&e))?;
    let mut sending = PictureSending::default();
    let mut next = sending.next(shown, |number| mirror.history_row(number));
    let mut frames = Vec::new();
    while let Some(picture) = next {
        frames.push(HubFrame::TerminalPicture { subscription_id: watch.subscription_id, picture });
        next = sending.rest();
    }
    Ok(Outcome::Reply(ClientResponse::Ok, frames))
}

pub(super) fn rows(world: &World, rows: &TerminalRows) -> Answer {
    let reply = mirror(world, &rows.id)?.history_rows(rows.numbering, rows.from, rows.count).map_err(|e| failed(&e))?;
    Ok(Outcome::Reply(ClientResponse::TerminalRows(reply), Vec::new()))
}

pub(super) fn bounds(world: &World, bounds: &TerminalBounds) -> Answer {
    let reply = mirror(world, &bounds.id)?.bounds(bounds.numbering, bounds.at, bounds.of).map_err(|e| failed(&e))?;
    Ok(Outcome::Reply(ClientResponse::TerminalBounds(reply), Vec::new()))
}

pub(super) fn text(world: &World, text: &TerminalText) -> Answer {
    let reply = mirror(world, &text.id)?.text(text.numbering, text.range, text.rectangle).map_err(|e| failed(&e))?;
    Ok(Outcome::Reply(ClientResponse::TerminalText(reply), Vec::new()))
}

fn screen<'w>(world: &'w World, id: &TerminalId) -> Result<&'w TerminalScreen, ApiError> {
    world
        .terminals
        .iter()
        .find(|(terminal, _)| terminal.id == *id)
        .map(|(_, screen)| screen)
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("a fake hub has no terminal {id}")))
}

/// The world's terminal, as a worker would keep it, from the bytes that draw it.
fn mirror(world: &World, id: &TerminalId) -> Result<Mirror, ApiError> {
    let screen = screen(world, id)?;
    let mut mirror = Mirror::new(screen.size).map_err(|e| failed(&e))?;
    mirror.write(screen.bytes.as_slice());
    Ok(mirror)
}

fn failed(e: &MirrorError) -> ApiError {
    ApiError::new(ErrorCode::Internal, format!("a fake hub couldn't read its terminal: {e}"))
}
