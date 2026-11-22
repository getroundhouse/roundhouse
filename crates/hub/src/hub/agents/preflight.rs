//! Ask about a prospective launch. A preflight command answers once, with no workspace or session to manage.

use protocol::capability::settings::ModelCatalog;
use protocol::client::{ApiError, ErrorCode, Outcome};
use protocol::model::{AgentKind, AgentLogin, RepoOrigin};
use protocol::preflight::{Asked, Question};
use protocol::provider::OpenPreflight;
use providers::Provider;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::super::Hub;
use crate::child_process::{self, Stderr};
use crate::error::provider_failed;

/// Bounds the agent's CLI starting and giving every page of what it offers, together.
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(45);

impl Hub {
    pub(super) async fn preflight_models(
        &self,
        provider: &Arc<dyn Provider>,
        project: RepoOrigin,
        agent: AgentKind,
        login: Option<AgentLogin>,
    ) -> Result<ModelCatalog, ApiError> {
        let command =
            provider.open_preflight(&OpenPreflight { project, agent }).await.map_err(|e| provider_failed(&e))?;
        let given = self.machine.given(&command.env).await;
        let mut child = child_process::spawn(&command.program, &command.args, &given, Stderr::Hubs)
            .map_err(|e| failed(e.to_string()))?;
        let exchange = async {
            let asked = Asked { version: protocol::preflight::VERSION, question: Question::Models { login } };
            let frame = protocol::frame::encode(&asked).map_err(|e| failed(e.to_string()))?;
            child.writer.write_all(&frame).await.map_err(|e| failed(e.to_string()))?;
            let mut reply = Vec::new();
            BufReader::new(child.reader)
                .take(protocol::frame::MAX_FRAME_BYTES as u64 + 1)
                .read_until(b'\n', &mut reply)
                .await
                .map_err(|e| failed(e.to_string()))?;
            if reply.len() > protocol::frame::MAX_FRAME_BYTES {
                return Err(failed("preflight answer is too large"));
            }
            if reply.is_empty() {
                return Err(failed("preflight ended without an answer"));
            }
            match protocol::frame::decode(&reply).map_err(|e| failed(e.to_string()))? {
                Outcome::Response(catalog) => Ok(catalog),
                Outcome::Error(error) => Err(error),
            }
        };
        let result = tokio::time::timeout(PREFLIGHT_TIMEOUT, exchange).await;
        drop(child.writer);
        child.process.end(crate::worker_link::STOP_GRACE).await;
        result.map_err(|_| failed("timed out waiting for model preflight"))?
    }
}

fn failed(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Worker, message)
}
