use serde::{Deserialize, Serialize};

use super::TicketSecret;
use crate::model::InstanceId;

/// What a worker's connection to its hub says it speaks, as iroh's ALPN. The worker protocol's own hello says which
/// version of it, so this changes only if what comes before the hello does.
pub const WORKER_ALPN: &[u8] = b"roundhouse/worker";

/// The first frame a worker sends on its connection, with the secret from its ticket.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Admission {
    pub secret: TicketSecret,
}

/// What the hub admitted the worker as. A refusal is an `ErrorCode::Unauthorized`, after which the hub closes the
/// connection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub instance_id: InstanceId,
}
