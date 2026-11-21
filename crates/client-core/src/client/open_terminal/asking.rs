//! What a terminal's window asks of the hub: sent at once, in order, from the thread the terminal is followed on, so a
//! key pressed after a new size reaches a program that has it, and each reply waited for on a thread of its own, one
//! at a time, and given back, so the terminal's thread goes on taking pictures meanwhile.

use protocol::client::terminals::{TerminalBounds, TerminalFit, TerminalRows};
use protocol::model::TerminalId;
use protocol::terminal::{Numbering, TerminalPoint, TerminalRange, TerminalSize};

use super::following::{Given, Held};
use super::window::{Ask, Replied, Step};
use crate::client::follow::Giver;
use crate::client::sent_call::SentCall;
use crate::client::{Client, Result};

/// A reply to wait for, or calls for the thread to make itself.
pub(super) enum Waiting {
    Rows(SentCall<TerminalRows>),
    Fit {
        size: TerminalSize,
        asked: Vec<TerminalPoint>,
        sent: SentCall<TerminalFit>,
    },
    Bounds {
        step: Step,
        sent: SentCall<TerminalBounds>,
    },
    /// The text of what's selected, a page at a time, to send to `reply` once it's all come.
    Copy {
        client: Client,
        numbering: Numbering,
        range: TerminalRange,
        reply: flume::Sender<String>,
    },
}

/// Sends what the window asked for, and gives back what has a reply to wait for. What fails to send is dropped, as
/// typing over a connection that's gone is, and the window asks again for what it still lacks.
pub(super) fn send(client: &Client, id: &TerminalId, ask: Ask) -> Result<Option<Waiting>> {
    let id = id.clone();
    Ok(Some(match ask {
        Ask::Typed(typed) => {
            client.terminal_typed(&id, typed)?;
            return Ok(None);
        }
        Ask::Rows { numbering, from, count } => {
            Waiting::Rows(client.send_call(TerminalRows { id, numbering, from, count })?)
        }
        Ask::Fit { size, numbering, keep } => {
            let sent = client.send_call(TerminalFit { id, size, numbering, keep: keep.clone() })?;
            Waiting::Fit { size, asked: keep, sent }
        }
        Ask::Bounds { numbering, at, of, step } => {
            Waiting::Bounds { step, sent: client.send_call(TerminalBounds { id, numbering, at, of })? }
        }
    }))
}

/// Waits for each reply given to `waiting`, in order, and gives it to the terminal, until the terminal's thread lets
/// go of it.
pub(super) fn start(id: TerminalId, waiting: flume::Receiver<Waiting>, giver: Giver<Held>) {
    let wait = move || {
        while let Ok(next) = waiting.recv() {
            let replied = match next {
                Waiting::Rows(sent) => sent.reply().map(Replied::Rows),
                Waiting::Fit { size, asked, sent } => sent.reply().map(|reply| Replied::Fit { size, asked, reply }),
                Waiting::Bounds { step, sent } => sent.reply().map(|reply| Replied::Bounds { step, reply }),
                Waiting::Copy { client, numbering, range, reply } => {
                    // What fails copies nothing, as a copy with nothing selected does.
                    if let Ok(text) = copied(&client, &id, numbering, range) {
                        let _ = reply.send(text);
                    }
                    continue;
                }
            };
            if !giver.give(|held| held.given.push(Given::Replied(replied.unwrap_or(Replied::Failed)))) {
                return;
            }
        }
    };
    // Without it, the window waits for no reply, and asks for nothing more of the hub.
    let _ = std::thread::Builder::new().name("terminal replies".to_string()).spawn(wait);
}

/// The text of `range`, from every page of it, joined as the replies say.
fn copied(client: &Client, id: &TerminalId, numbering: Numbering, range: TerminalRange) -> Result<String> {
    let mut text = String::new();
    let mut from = range.from;
    loop {
        let page = client.terminal_text(id, numbering, TerminalRange { from, ..range }, false)?;
        text.push_str(&page.text);
        let Some(rest) = page.rest else { return Ok(text) };
        if !page.joins {
            text.push('\n');
        }
        from = rest;
    }
}
