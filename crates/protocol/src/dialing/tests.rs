use super::*;

fn ticket() -> Ticket {
    Ticket {
        hub: HubAddress {
            key: "ae58ff8833241ac82d6ff7611046ed67b5072d142c588d0063e942d9a75502b6".into(),
            relay: Some("https://relay.example/".into()),
            direct: vec!["192.168.1.20:7868".parse().unwrap()],
        },
        secret: TicketSecret::from("c2VjcmV0".to_string()),
    }
}

/// So an adapter can put it on a command line as it is, and the worker reads back what the hub made.
#[test]
fn a_ticket_is_one_word_that_reads_back_as_itself() {
    let given = WorkerTicket::from(&ticket());

    assert!(given.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{}", given.as_str());
    assert_eq!(given.read(), Ok(ticket()));
}

#[test]
fn what_isnt_a_ticket_says_so() {
    assert_eq!(WorkerTicket::from("not a ticket!".to_string()).read(), Err(TicketError::NotBase64));
    let json_of_another = WorkerTicket::from(base64_of(r#"{"hub":"elsewhere"}"#));
    assert!(matches!(json_of_another.read(), Err(TicketError::NotATicket(_))), "{:?}", json_of_another.read());
}

/// So nothing that logs what holds one logs what admits a worker.
#[test]
fn a_ticket_and_its_secret_are_never_in_what_debug_writes() {
    let ticket = ticket();

    assert!(!format!("{ticket:?}").contains(ticket.secret.as_str()));
    assert_eq!(format!("{:?}", WorkerTicket::from(&ticket)), "WorkerTicket(..)");
}

fn base64_of(text: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}
