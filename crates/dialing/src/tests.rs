use super::*;

/// So a worker can dial the hub again after it restarts, at the key and the port its ticket names.
#[tokio::test]
async fn a_hub_endpoint_keeps_its_key_and_its_port() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("endpoint.json");

    let first = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap();
    let address = first.address().await;
    first.close().await;
    drop(first);
    let again = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap().address().await;

    assert_eq!(again, address);
    assert!(address.relay.is_none(), "{address:?}");
    assert!(address.direct.iter().all(|addr| addr.ip().is_loopback()) && !address.direct.is_empty(), "{address:?}");
}

/// So no other program's socket, given a port by the system, takes the port while the hub restarts, and the workers on
/// its own machine, which have no relay to fall back on, still find it.
#[tokio::test]
async fn a_loopback_endpoint_binds_a_port_the_system_gives_no_ephemeral_socket() {
    let dir = tempfile::tempdir().unwrap();

    let address = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None, LOOPBACK_PORTS)
        .await
        .unwrap()
        .address()
        .await;

    let ports: Vec<u16> = address.direct.iter().map(std::net::SocketAddr::port).collect();
    assert!(!ports.is_empty() && ports.iter().all(|port| LOOPBACK_PORTS.contains(*port)), "{ports:?}");
}

/// A port taken while the hub was gone, here by the endpoint that had it, is given up for another of the same range,
/// which tickets name from then on.
#[tokio::test]
async fn a_loopback_endpoint_whose_port_was_taken_binds_another_of_its_range() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("endpoint.json");
    let first = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap();
    let kept = first.address().await.direct[0];

    let again = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap().address().await;

    let port = again.direct[0].port();
    assert_ne!(port, kept.port());
    assert!(LOOPBACK_PORTS.contains(port), "{port}");
}

/// The hub knows a worker by the key it proves, which is the one it binds with.
#[tokio::test]
async fn a_worker_dials_the_address_a_ticket_names_and_the_hub_sees_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None, LOOPBACK_PORTS).await.unwrap();
    let worker = WorkerEndpoint::bind(&hub.address().await, None, None).await.unwrap();

    let (dialed, accepted) = tokio::join!(worker.dial(), async { hub.accept().await.unwrap().await.unwrap() });

    assert_eq!(remote_key(&dialed.unwrap()), hub.address().await.key);
    assert_eq!(remote_key(&accepted), worker.key());
}

/// A worker whose hub is on its own machine, on loopback, stays off the network too, so no firewall asks about it.
#[tokio::test]
async fn a_worker_whose_hub_is_on_loopback_binds_only_to_loopback() {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None, LOOPBACK_PORTS).await.unwrap();

    let worker = WorkerEndpoint::bind(&hub.address().await, None, None).await.unwrap();

    let bound = worker.endpoint.bound_sockets();
    assert!(!bound.is_empty() && bound.iter().all(|socket| socket.ip().is_loopback()), "{bound:?}");
}

/// So a worker finds a hub that came back on another port, as the address file the hub writes says, and dials only
/// the hub its ticket names, wherever the file says one is.
#[tokio::test]
async fn a_worker_dials_where_the_address_file_says_its_hub_is_and_no_other_hub() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("endpoint.json");
    let first = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap();
    let ticket = first.address().await;
    let worker = WorkerEndpoint::bind(&ticket, None, None).await.unwrap();
    // The next hub, with the same key, while the port it kept is held.
    let next = HubEndpoint::bind(&file, Relays::None, LOOPBACK_PORTS).await.unwrap();
    first.close().await;
    drop(first);
    let address_file = dir.path().join("address.json");
    address_file::write(&address_file, &next.address().await).unwrap();
    let other = HubEndpoint::bind(&dir.path().join("other.json"), Relays::None, LOOPBACK_PORTS).await.unwrap();

    let refused = worker.moved_to(&other.address().await);
    let moved = worker.moved_to(&address_file::read(&address_file).unwrap()).unwrap();
    let again = worker.moved_to(&address_file::read(&address_file).unwrap()).unwrap();
    let (dialed, accepted) = tokio::join!(worker.dial(), async { next.accept().await.unwrap().await.unwrap() });

    assert!(matches!(refused, Err(DialError::OtherHub { .. })), "{refused:?}");
    assert!(moved, "the next hub is somewhere else");
    assert!(!again, "and it's there still");
    assert_eq!(remote_key(&dialed.unwrap()), ticket.key);
    assert_eq!(remote_key(&accepted), worker.key());
}

/// So a hub is never given no port to bind, which it would take for some.
#[test]
fn a_range_of_no_ports_is_no_ports_to_bind() {
    assert!(Ports::new(20_000..20_000).is_none());
    let (start, end) = (20_001, 20_000);
    assert!(Ports::new(start..end).is_none(), "one that ends before it starts");
    assert!(Ports::new(20_000..20_001).is_some_and(|ports| ports.contains(20_000)));
}
