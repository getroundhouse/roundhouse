//! A peer can complete TLS and then stop processing requests while its TCP connection stays alive.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use client_core::{Client, ClientError, RemoteHub};
use protocol::client::{ClientFrame, ClientResponse, HubFrame, ReplyOutcome, devices};
use protocol::pairing::{DeviceToken, Fingerprint, PairingCode, PairingLink};
use tokio_rustls::rustls::{self, ServerConfig, ServerConnection, StreamOwned};

const TEST_DEADLINE: Duration = Duration::from_secs(5);
const OPERATION_TIMEOUT: Duration = Duration::from_millis(200);

/// Holds the server socket open until the assertion has an answer, even if the client stopped reading it.
struct Peer {
    hub: RemoteHub,
    release: mpsc::Sender<()>,
    task: std::thread::JoinHandle<()>,
}

impl Peer {
    fn silent(after_authentication: bool) -> Self {
        let key = rcgen::KeyPair::generate().unwrap();
        let certificate = rcgen::CertificateParams::new(vec!["roundhouse".into()]).unwrap().self_signed(&key).unwrap();
        let spki = rcgen::PublicKeyData::subject_public_key_info(&key);
        let mut digest = [0; 32];
        digest.copy_from_slice(ring::digest::digest(&ring::digest::SHA256, &spki).as_ref());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
            )
            .unwrap();
        let listener = small_window_listener();
        let hub = RemoteHub {
            hub_name: "Silent hub".into(),
            hosts: vec![listener.local_addr().unwrap().to_string()],
            fingerprint: Fingerprint::of_sha256(digest),
            device_id: "test-device".into(),
            token: DeviceToken::from("test-token"),
        };
        let (release, done) = mpsc::channel();
        let task = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(TEST_DEADLINE)).unwrap();
            let mut tls = ServerConnection::new(Arc::new(config)).unwrap();
            while tls.is_handshaking() {
                tls.complete_io(&mut socket).unwrap();
            }
            let mut stream = BufReader::new(StreamOwned::new(tls, socket));
            if after_authentication {
                let mut line = Vec::new();
                stream.read_until(b'\n', &mut line).unwrap();
                let request: ClientFrame = protocol::frame::decode(&line).unwrap();
                assert!(matches!(request.request, protocol::client::ClientRequest::Authenticate(_)));
                let reply = HubFrame::Reply { id: request.id, outcome: ReplyOutcome::Response(ClientResponse::Ok) };
                stream.get_mut().write_all(&protocol::frame::encode(&reply).unwrap()).unwrap();
                stream.get_mut().flush().unwrap();
            }
            let _ = done.recv_timeout(TEST_DEADLINE + Duration::from_secs(5));
            drop(stream);
        });
        Self { hub, release, task }
    }

    fn finish(self) {
        let _ = self.release.send(());
        self.task.join().unwrap();
    }
}

/// A listener whose connections take little before a peer that doesn't read stops the writer: Linux holds 2.8 MB on a
/// loopback connection by default, and 1.2 MB with this. A Mac keeps its own size, which holds at most 1.4 MB on CI's
/// runners, as rustls writes. Set before listening, so connections take it as they're accepted.
fn small_window_listener() -> TcpListener {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
    socket.set_recv_buffer_size(4096).unwrap();
    socket.bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 0)).into()).unwrap();
    socket.listen(1).unwrap();
    socket.into()
}

fn finishes<T: Send + 'static>(peer: Peer, call: impl FnOnce(RemoteHub) -> T + Send + 'static) -> T {
    let hub = peer.hub.clone();
    let (sent, result) = mpsc::channel();
    let task = std::thread::spawn(move || {
        let _ = sent.send(call(hub));
    });
    let answer = result.recv_timeout(TEST_DEADLINE);
    peer.finish();
    task.join().unwrap();
    answer.expect("a live TCP peer held the client beyond the deadline")
}

#[test]
fn authentication_cannot_wait_forever_after_tls_succeeds() {
    let error = finishes(Peer::silent(false), |hub| {
        Client::connect_remote_with_timeout(&hub, "test", OPERATION_TIMEOUT).err().unwrap()
    });
    assert!(matches!(error, ClientError::Io(ref e) if e.kind() == std::io::ErrorKind::TimedOut), "{error}");
}

#[test]
fn hello_cannot_wait_forever_after_authentication_succeeds() {
    let error = finishes(Peer::silent(true), |hub| {
        Client::connect_remote_with_timeout(&hub, "test", OPERATION_TIMEOUT).err().unwrap()
    });
    assert!(matches!(error, ClientError::Io(ref e) if e.kind() == std::io::ErrorKind::TimedOut), "{error}");
}

#[test]
fn pairing_cannot_wait_forever_after_tls_succeeds() {
    let error = finishes(Peer::silent(false), |hub| {
        let link = PairingLink {
            hub_name: hub.hub_name,
            hosts: hub.hosts,
            fingerprint: hub.fingerprint,
            code: PairingCode::from("test-code"),
        };
        client_core::pair_with_timeout(&link, "test", OPERATION_TIMEOUT).unwrap_err()
    });
    assert!(matches!(error, ClientError::Io(ref e) if e.kind() == std::io::ErrorKind::TimedOut), "{error}");
}

#[test]
fn a_peer_that_stops_reading_disconnects_all_pending_calls() {
    finishes(Peer::silent(false), |hub| {
        let client = Client::open_remote_with_timeout(&hub.hosts, &hub.fingerprint, OPERATION_TIMEOUT).unwrap();
        let waiting = client.start(Client::records);
        // About three times what the connection holds while the peer reads no application data. No more, since the
        // deadline counts encoding it, which a test build does slowly: on a Mac runner busy with other tests, 16 MB
        // took 4.8 s, and on an idle Mac 0.2 s.
        let error = client
            .call(devices::Pair { code: PairingCode::from("test-code"), device_name: "x".repeat(4 * 1024 * 1024) })
            .unwrap_err();
        assert!(matches!(error, ClientError::Disconnected), "{error}");
        assert!(client.is_disconnected());
        assert!(tokio::runtime::Runtime::new().unwrap().block_on(waiting.recv()).is_err());
        assert!(client.records().is_err());
    });
}
