//! What a worker sends its hub before the worker protocol's hello, and the ticket it dials with. A worker that outlives
//! its hub dials the next one, which can be a newer build, so these keep their forms.

use protocol::client::Outcome;
use protocol::dialing::{Admission, Admitted, HubAddress, Ticket, TicketSecret, WorkerTicket};

use crate::keeps_its_form;

#[test]
fn what_a_worker_says_as_it_dials_keeps_its_form() {
    keeps_its_form(&Admission { secret: TicketSecret::from("c2VjcmV0".to_string()) }, r#"{"secret":"c2VjcmV0"}"#);
    keeps_its_form(&Outcome::Response(Admitted { instance_id: "i".into() }), r#"{"response":{"instance_id":"i"}}"#);
}

#[test]
fn a_ticket_keeps_its_form() {
    let ticket = Ticket {
        hub: HubAddress {
            key: "ae58".into(),
            relay: Some("https://relay.example/".into()),
            direct: vec!["192.168.1.20:7868".parse().unwrap()],
        },
        secret: TicketSecret::from("c2VjcmV0".to_string()),
    };
    let json =
        r#"{"hub":{"key":"ae58","relay":"https://relay.example/","direct":["192.168.1.20:7868"]},"secret":"c2VjcmV0"}"#;

    keeps_its_form(&ticket, json);
    let given = WorkerTicket::from(&ticket);
    assert_eq!(given.read(), Ok(ticket));
    assert_eq!(given, WorkerTicket::from(base64_of(json)));
}

fn base64_of(text: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}
