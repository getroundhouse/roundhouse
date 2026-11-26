//! Against a stand-in hub on a Unix socket: one that saves, and one from before saves, which has the server checked
//! and then added or edited. What's let go of sends nothing more, and its cancel follows its save. One that has been
//! answered has nothing to let go of.

#![cfg(unix)]

use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::tool_server_checks::{NoToolList, ToolServerChecked};
use protocol::client::tool_server_saves::ToolServerSaved;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::model::ToolServerAuthGiven;

use super::super::Client;
use super::ToolServerSaving;
use super::saving::Saving;
use crate::stand_in_hub::{self, Does};
use crate::tool_server_checks::{Saved, ToSave, When};

const URL: &str = "https://mcp.example.com/mcp";
const WAIT: Duration = Duration::from_secs(10);

fn answers(response: ClientResponse) -> Does {
    Does::Answers(Box::new(ReplyOutcome::Response(response)))
}

fn unsupported() -> Does {
    Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "a hub from before saves"))))
}

fn add() -> ToSave {
    let auth = ToolServerAuthGiven::Neither;
    ToSave::Add(ToolServerAdd { id: "t".into(), name: "Runbooks".into(), url: URL.into(), auth })
}

fn edit() -> ToSave {
    ToSave::Edit(ToolServerEdit { id: "t".into(), name: None, url: Some(URL.into()), auth: None, saved_anyway: false })
}

/// What each request but a hello asks, in a few words.
fn said(request: &ClientRequest) -> Option<String> {
    #[expect(clippy::wildcard_enum_match_arm, reason = "any other request is said as it is")]
    Some(match request {
        ClientRequest::Hello(_) => return None,
        ClientRequest::ToolServerSave(save) => format!("save {:?}", save.when),
        ClientRequest::ToolServerSaveCancel(_) => "cancel".into(),
        ClientRequest::ToolServerCheck(_) => "check".into(),
        ClientRequest::ToolServerAdd(_) => "add".into(),
        ClientRequest::ToolServerEdit(edit) => format!("edit, saved anyway: {}", edit.saved_anyway),
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

/// Starts saving `of` as `when` says, and lets go of it once it has been answered.
fn saved(client: &Client, of: ToSave, when: When, name: &str) -> Saved {
    let (saving, reply) = ToolServerSaving::start(client, of, when, name.into(), URL.into());
    let saved = reply.wait().unwrap();
    drop(saving);
    saved
}

/// Checks answered as listed, then as not, and one saved anyway, which isn't checked. Each is let go of once it has
/// been answered, which sends nothing.
#[test]
fn a_hub_from_before_saves_has_it_checked_then_added_or_edited_and_one_saved_anyway_isnt_checked() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let record = protocol::fixtures::tool_server("t");
    let checks = Mutex::new(vec![
        ToolServerChecked::NotListed { why: NoToolList::KeyRefused { status: 401 } },
        ToolServerChecked::Listed { tools: 2 },
    ]);
    let server = record.clone();
    let from = stand_in_hub::listen(&socket, move |request| {
        #[expect(clippy::wildcard_enum_match_arm, reason = "it answers an add or an edit with the server's record")]
        match request {
            ClientRequest::ToolServerSave(_) | ClientRequest::ToolServerSaveCancel(_) => unsupported(),
            ClientRequest::ToolServerCheck(_) => {
                answers(ClientResponse::ToolServerChecked(checks.lock().pop().unwrap()))
            }
            _ => answers(ClientResponse::ToolServer(server.clone())),
        }
    });
    let client = Client::connect(&socket, "test").unwrap();

    let added = saved(&client, add(), When::Listed, " Runbooks ");
    let refused = saved(&client, edit(), When::Listed, "Runbooks");
    let anyway = saved(&client, edit(), When::Anyway, "Runbooks");

    assert_eq!((added, anyway), (Saved::Saved(record.clone()), Saved::Saved(record)));
    assert_eq!(refused, Saved::NotListed("Runbooks refused this key.".into()));
    let heard = Heard { from, so_far: Vec::new() }.all();
    let expected = ["save Listed", "check", "add", "save Listed", "check", "save Anyway", "edit, saved anyway: true"];
    assert_eq!(heard, expected);
}

/// Its check is held until it has been let go of.
#[test]
fn a_save_let_go_of_while_a_hub_from_before_saves_checks_it_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (release, held) = flume::bounded(1);
    let listed = answers(ClientResponse::ToolServerChecked(ToolServerChecked::Listed { tools: 2 }));
    let mut listed = Some(Does::After(held, Box::new(listed)));
    let from = stand_in_hub::listen(&socket, move |request| {
        #[expect(clippy::wildcard_enum_match_arm, reason = "a hub from before saves refuses them")]
        match request {
            ClientRequest::ToolServerCheck(_) => listed.take().unwrap(),
            _ => unsupported(),
        }
    });
    let client = Client::connect(&socket, "test").unwrap();
    let mut heard = Heard { from, so_far: Vec::new() };
    let (saving, reply) = ToolServerSaving::start(&client, add(), When::Listed, "Runbooks".into(), URL.into());
    heard.until("check");

    drop(saving);
    heard.until("cancel");
    release.send(()).unwrap();

    assert_eq!(reply.wait().unwrap(), Saved::LetGo);
    assert_eq!(heard.all(), ["save Listed", "check", "cancel"], "and no add");
}

/// Let go of before it's sent, then one let go of while the hub, which holds its answer, asks the server.
#[test]
fn a_save_let_go_of_before_its_sent_sends_nothing_and_one_let_go_of_after_has_its_cancel_follow_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (release, held) = flume::bounded(1);
    let cancelled = answers(ClientResponse::ToolServerSaved(ToolServerSaved::Cancelled));
    let mut cancelled = Some(Does::After(held, Box::new(cancelled)));
    let from = stand_in_hub::listen(&socket, move |request| {
        #[expect(clippy::wildcard_enum_match_arm, reason = "it takes a cancel")]
        match request {
            ClientRequest::ToolServerSave(_) => cancelled.take().unwrap(),
            _ => answers(ClientResponse::Ok),
        }
    });
    let client = Client::connect(&socket, "test").unwrap();
    let mut heard = Heard { from, so_far: Vec::new() };

    let early = Saving::default();
    early.let_go(&client).unwrap();
    let not_sent = early.save(&client, add(), When::Listed, "Runbooks", URL).unwrap();
    let (saving, reply) = ToolServerSaving::start(&client, add(), When::Listed, "Runbooks".into(), URL.into());
    heard.until("save Listed");
    drop(saving);
    heard.until("cancel");
    release.send(()).unwrap();

    assert_eq!((not_sent, reply.wait().unwrap()), (Saved::LetGo, Saved::LetGo));
    assert_eq!(heard.all(), ["save Listed", "cancel"]);
}
