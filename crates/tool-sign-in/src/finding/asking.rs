use crate::challenge::Challenge;
use crate::fetch::Caller;
use crate::messages::{Message, refuses_what_was_asked};
use crate::{Failure, ServerUrl};

/// Asks `server` something as nobody, as a client of this version would first, and, where it refuses that for what was
/// asked rather than who asked, as one of the last version with a handshake would, the order MCP gives. A server may
/// check either before it asks for a sign-in, so a refusal of one doesn't say that it asks for none. Gives what it says
/// as it asks for a sign-in, or nothing where it answers anyone.
pub(super) async fn as_nobody(calls: &Caller<'_>, server: &ServerUrl) -> Result<Option<Challenge>, Failure> {
    let ask = async |message: Message| calls.post_message(server.url(), message.headers, &message.body).await;
    let answer = ask(Message::discover()).await?;
    let answer = if refuses_what_was_asked(answer.status) { ask(Message::initialize()).await? } else { answer };
    if answer.succeeded() {
        return Ok(None);
    }
    match answer.status {
        401 => Ok(Some(Challenge::of(&answer.headers))),
        _ => Err(answer.failure()),
    }
}
