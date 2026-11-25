//! What the hub logs of a connection's requests.

use logging::testing::Logs;
use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientRequest, ReplyOutcome};
use serde_json::json;

use super::connection::Connection;
use crate::hub::tests::hub_in;

#[tokio::test]
async fn what_a_request_logs_says_which_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut connection = Connection::greeted(&hub, CLIENT_PROTOCOL_VERSION).await;
    let database = rusqlite::Connection::open(dir.path().join("hub.db")).unwrap();
    database.execute_batch("ALTER TABLE devices RENAME TO no_devices").unwrap();
    let logs = Logs::capture();

    let outcome = connection.reply(7, &ClientRequest::DeviceList).await;

    assert!(matches!(outcome, ReplyOutcome::Error(_)), "{outcome:?}");
    let failed: Vec<_> = logs.events().into_iter().filter(|event| event["event"] == "database.failed").collect();
    let [failed] = &failed[..] else { panic!("{}", logs.text()) };
    assert_eq!(failed["context"]["call"], json!(7));
    assert!(failed["context"]["connection"].is_u64(), "{failed}");
}
