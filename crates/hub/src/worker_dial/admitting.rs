use std::sync::Arc;
use std::time::Duration;

use dialing::{HubEndpoint, Incoming, read_frame, remote_key, write_frame};
use protocol::client::{ApiError, ErrorCode, Outcome};
use protocol::dialing::{Admission, Admitted};
use tracing::{info, warn};

use super::Dialed;
use super::arrivals::Arrivals;
use crate::store::Store;

/// How long a worker that has connected has to say what admits it, after which the hub closes the connection, so one
/// that never says doesn't hold it open.
const ADMISSION_WITHIN: Duration = Duration::from_secs(10);

/// What the hub closes a connection it refused with, as QUIC's application error code.
const REFUSED: u32 = 1;

/// Admits each worker that dials, each on a task of its own, so a slow one holds up no other, until the endpoint
/// closes.
pub(super) async fn admit(endpoint: Arc<HubEndpoint>, store: Arc<Store>, arrivals: Arc<Arrivals>) {
    while let Some(incoming) = endpoint.accept().await {
        let (store, arrivals) = (store.clone(), arrivals.clone());
        tokio::spawn(async move {
            if let Err(e) = admit_one(incoming, store, &arrivals).await {
                warn!(name: "dialed.failed", error = e.message, "A worker's connection to the hub failed");
            }
        });
    }
}

async fn admit_one(incoming: Incoming, store: Arc<Store>, arrivals: &Arrivals) -> Result<(), ApiError> {
    let failed = |e: &dyn std::fmt::Display| ApiError::new(ErrorCode::Worker, e.to_string());
    let connection = incoming.await.map_err(|e| failed(&e))?;
    let key = remote_key(&connection);
    let said = tokio::time::timeout(ADMISSION_WITHIN, async {
        let (send, mut recv) = connection.accept_bi().await.map_err(|e| failed(&e))?;
        let admission: Admission = read_frame(&mut recv).await.map_err(|e| failed(&e))?;
        Ok::<_, ApiError>((send, recv, admission))
    })
    .await;
    let Ok(said) = said else {
        connection.close(REFUSED.into(), b"said nothing within the time allowed");
        return Err(ApiError::new(ErrorCode::Worker, format!("a worker said nothing within {ADMISSION_WITHIN:?}")));
    };
    let (mut send, recv, admission) = said?;
    let owned = key.clone();
    let admitted = tokio::task::spawn_blocking(move || store.admit_worker(&admission.secret, &owned))
        .await
        .map_err(|e| failed(&e))??;
    let Some(instance_id) = admitted else {
        let refused = ApiError::new(ErrorCode::Unauthorized, "the ticket has been used, or was never the hub's");
        // So the worker reads why before the connection closes.
        let _ = write_frame(&mut send, &Outcome::<Admitted>::Error(refused.clone())).await;
        let _ = send.finish();
        let _ = tokio::time::timeout(Duration::from_secs(1), send.stopped()).await;
        connection.close(REFUSED.into(), b"refused");
        warn!(name: "dialed.refused", worker = %key, "Refused a worker whose ticket the hub doesn't hold");
        return Ok(());
    };
    write_frame(&mut send, &Outcome::Response(Admitted { instance_id: instance_id.clone() }))
        .await
        .map_err(|e| failed(&e))?;
    info!(name: "dialed.admitted", %instance_id, worker = %key, "Admitted a worker that dialed");
    arrivals.arrive(Dialed { instance_id, connection, send, recv });
    Ok(())
}
