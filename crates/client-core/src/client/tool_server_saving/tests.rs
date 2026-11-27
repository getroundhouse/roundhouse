//! Against a stand-in hub on a Unix socket. What's let go of sends nothing more, and its cancel follows its save. One
//! that has been answered has nothing to let go of.

#![cfg(unix)]

use std::time::Duration;

use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSaved};
use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome};
use protocol::model::ToolServerAuthGiven;

use super::super::Client;
use super::ToolServerSaving;
use super::saving::Saving;
use crate::stand_in_hub::{self, Does};
use crate::tool_server_checks::Saved;

const URL: &str = "https://mcp.example.com/mcp";
const WAIT: Duration = Duration::from_secs(10);

fn answers(response: ClientResponse) -> Does {
    Does::Answers(Box::new(ReplyOutcome::Response(response)))
}

fn add() -> ToolServerToCheck {
    let auth = ToolServerAuthGiven::Neither;
    ToolServerToCheck::Add(ToolServerAdd { id: "t".into(), name: "Runbooks".into(), url: URL.into(), auth })
}

/// What each request but a hello asks, in a few words.
fn said(request: &ClientRequest) -> Option<String> {
    #[expect(clippy::wildcard_enum_match_arm, reason = "any other request is said as it is")]
    Some(match request {
        ClientRequest::Hello(_) => return None,
        ClientRequest::ToolServerSave(save) => format!("save {:?}", save.when),
        ClientRequest::ToolServerSaveCancel(_) => "cancel".into(),
        other => format!("{other:?}"),
    })
}

/// What's been heard so far.
struct Heard {
    from: flume::Receiver<ClientRequest>,
    so_far: Vec<String>,
}

impl Heard {
    fn until(&mut self, what: &str) {
        let Self { from, so_far } = self;
        waiting::thread::until(WAIT, format!("{what} wasn't heard"), || {
            so_far.extend(from.try_iter().filter_map(|request| said(&request)));
            so_far.iter().any(|heard| heard == what)
        });
    }

    fn all(&mut self) -> Vec<String> {
        self.so_far.extend(self.from.try_iter().filter_map(|request| said(&request)));
        std::mem::take(&mut self.so_far)
    }
}

/// Let go of before it's sent, then one let go of while the hub, which holds its answer, asks the server, then one let
/// go of once it has been answered.
#[test]
fn a_save_let_go_of_before_its_sent_sends_nothing_and_one_let_go_of_after_has_its_cancel_follow_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (release, held) = flume::bounded(1);
    let cancelled = answers(ClientResponse::ToolServerSaved(ToolServerSaved::Cancelled));
    let mut cancelled = Some(Does::After(held, Box::new(cancelled)));
    let record = protocol::fixtures::tool_server("t");
    let server = record.clone();
    let from = stand_in_hub::listen(&socket, move |request| {
        #[expect(clippy::wildcard_enum_match_arm, reason = "it takes a cancel")]
        match request {
            ClientRequest::ToolServerSave(_) => cancelled.take().unwrap_or_else(|| {
                answers(ClientResponse::ToolServerSaved(ToolServerSaved::Saved { server: server.clone() }))
            }),
            _ => answers(ClientResponse::Ok),
        }
    });
    let client = Client::connect(&socket, "test").unwrap();
    let mut heard = Heard { from, so_far: Vec::new() };

    let early = Saving::default();
    early.let_go(&client).unwrap();
    let not_sent = early.save(&client, add(), SaveWhen::Listed, "Runbooks", URL).unwrap();
    let (saving, reply) = ToolServerSaving::on(&client, add(), SaveWhen::Listed, "Runbooks".into(), URL.into());
    heard.until("save Listed");
    drop(saving);
    heard.until("cancel");
    release.send(()).unwrap();

    assert_eq!((not_sent, reply.wait().unwrap()), (Saved::LetGo, Saved::LetGo));
    let answered = Saving::default();
    assert_eq!(answered.save(&client, add(), SaveWhen::Anyway, "Runbooks", URL).unwrap(), Saved::Saved(record));
    answered.let_go(&client).unwrap();
    assert_eq!(heard.all(), ["save Listed", "cancel", "save Anyway"]);
}
