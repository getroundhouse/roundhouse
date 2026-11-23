//! What a fake hub says of a terminal: what it shows, which never changes, at the size it has, which a view
//! can't change. It's made from the bytes the world gives, the same way on every call, so a picture's numbering and
//! the rows asked for in it agree.

use protocol::client::terminals::{TerminalBounds, TerminalRows, TerminalText};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubFrame};
use protocol::model::TerminalId;
use protocol::terminal::PictureSending;
use terminal_state::{Mirror, MirrorError};

use super::serve::{Answer, Outcome};
use super::world::{TerminalShows, World};

/// A whole picture of what the world's terminal `id` shows, in as many frames as it takes, as a subscription that
/// follows it is sent.
pub(super) fn pictures(world: &World, id: &TerminalId, subscription_id: u64) -> Result<Vec<HubFrame>, ApiError> {
    let mut mirror = mirror(world, id)?;
    let shown = mirror.shown().map_err(|e| failed(&e))?;
    let mut sending = PictureSending::default();
    let mut next = sending.next(shown, |number| mirror.history_row(number));
    let mut frames = Vec::new();
    while let Some(picture) = next {
        frames.push(HubFrame::TerminalPicture { subscription_id, picture });
        next = sending.rest();
    }
    Ok(frames)
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

fn screen<'w>(world: &'w World, id: &TerminalId) -> Result<&'w TerminalShows, ApiError> {
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
    mirror.write(&screen.bytes);
    Ok(mirror)
}

fn failed(e: &MirrorError) -> ApiError {
    ApiError::new(ErrorCode::Internal, format!("a fake hub couldn't read its terminal: {e}"))
}
