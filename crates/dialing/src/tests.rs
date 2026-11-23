use super::*;

/// So a worker can dial the hub again after it restarts, at the key and the port its ticket names.
#[tokio::test]
async fn a_hub_endpoint_keeps_its_key_and_its_port() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("endpoint.json");

    let first = HubEndpoint::bind(&file, Relays::None).await.unwrap();
    let address = first.address().await;
    first.close().await;
    drop(first);
    let again = HubEndpoint::bind(&file, Relays::None).await.unwrap().address().await;

    assert_eq!(again, address);
    assert!(address.relay.is_none(), "{address:?}");
    assert!(address.direct.iter().all(|addr| addr.ip().is_loopback()) && !address.direct.is_empty(), "{address:?}");
}

/// The hub knows a worker by the key it proves, which is the one it binds with.
#[tokio::test]
async fn a_worker_dials_the_address_a_ticket_names_and_the_hub_sees_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubEndpoint::bind(&dir.path().join("endpoint.json"), Relays::None).await.unwrap();
    let worker = WorkerEndpoint::bind(&hub.address().await, None, None).await.unwrap();

    let (dialed, accepted) = tokio::join!(worker.dial(), async { hub.accept().await.unwrap().await.unwrap() });

    assert_eq!(remote_key(&dialed.unwrap()), hub.address().await.key);
    assert_eq!(remote_key(&accepted), worker.key());
}
