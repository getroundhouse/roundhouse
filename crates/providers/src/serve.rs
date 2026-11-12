//! Serves a [`Provider`] over the provider protocol, on the adapter's stdin and stdout, so an adapter written in Rust
//! is an implementation of the trait and a call to [`serve`].

use std::io;
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::frame::{self, Call};
use protocol::model::HubId;
use protocol::provider::{
    PROVIDER_PROTOCOL_VERSION, ProviderFrame, ProviderOutcome, ProviderRequest, ProviderResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{Provider, ProviderError};

/// Makes the provider once the hub has said which hub it is, since an adapter works only with that hub's instances.
pub type MakeProvider = Box<dyn FnOnce(&HubId) -> Result<Arc<dyn Provider>, ProviderError> + Send>;

/// Until the hub closes `reader`. Each request is answered as it finishes, so a slow one, such as starting a VM,
/// doesn't hold up the rest. Replies still being worked out when the hub goes are dropped, since the next hub asks
/// again.
pub async fn serve<R, W>(make: MakeProvider, build: &str, reader: R, writer: W) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (frames, outgoing) = mpsc::channel::<ProviderFrame>(64);
    let writing = tokio::spawn(write_frames(writer, outgoing));
    let mut make = Some(make);
    let mut serving: Option<(HubId, Arc<dyn Provider>)> = None;
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        frame::clear_buffer(&mut line);
        let limit = u64::try_from(frame::MAX_FRAME_BYTES).unwrap_or(u64::MAX).saturating_add(1);
        if (&mut reader).take(limit).read_until(b'\n', &mut line).await? == 0 {
            break;
        }
        if line.len() > frame::MAX_FRAME_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the hub sent an oversized frame"));
        }
        let Call { id, request } = match frame::read_call::<ProviderRequest>(&line) {
            Ok(call) => call,
            Err(e) => {
                warn!("ignoring a request that didn't parse: {e}");
                continue;
            }
        };
        let reply = move |outcome| ProviderFrame::Reply { id, outcome };
        match (request, &serving) {
            (Ok(ProviderRequest::Hello { protocol_version, oldest_protocol_version, hub_id, .. }), _) => {
                let outcome = match hello(&mut make, &mut serving, protocol_version, oldest_protocol_version, hub_id) {
                    Ok(provider) => ProviderOutcome::Response(ProviderResponse::Hello {
                        protocol_version: PROVIDER_PROTOCOL_VERSION,
                        adapter: provider.name().to_string(),
                        build: build.to_string(),
                        capabilities: provider.capabilities(),
                    }),
                    Err(e) => ProviderOutcome::Error(e),
                };
                let _ = frames.send(reply(outcome)).await;
            }
            (_, None) => {
                let refused = ApiError::new(ErrorCode::VersionMismatch, "the first request must be hello");
                let _ = frames.send(reply(ProviderOutcome::Error(refused))).await;
            }
            // A newer hub's, so it's refused as one this adapter doesn't do, rather than leaving the hub waiting.
            (Err(e), Some(_)) => {
                let refused = ApiError::unsupported_request("this adapter", Some(&e));
                let _ = frames.send(reply(ProviderOutcome::Error(refused))).await;
            }
            (Ok(request), Some((_, provider))) => answer_on_a_task(provider.clone(), request, id, frames.clone()),
        }
    }
    writing.abort();
    Ok(())
}

/// So a slow request, such as starting a VM, doesn't hold up the rest, and one whose answer panics is still answered.
fn answer_on_a_task(
    provider: Arc<dyn Provider>,
    request: ProviderRequest,
    id: u64,
    frames: mpsc::Sender<ProviderFrame>,
) {
    tokio::spawn(async move {
        let answering = tokio::spawn(async move { answer(provider.as_ref(), request).await });
        let outcome = match answering.await {
            Ok(Ok(response)) => ProviderOutcome::Response(response),
            Ok(Err(e)) => ProviderOutcome::Error(e.to_api_error()),
            Err(e) => ProviderOutcome::Error(ApiError::new(ErrorCode::Internal, format!("answering: {e}"))),
        };
        let _ = frames.send(ProviderFrame::Reply { id, outcome }).await;
    });
}

async fn write_frames<W: AsyncWrite + Unpin>(mut writer: W, mut frames: mpsc::Receiver<ProviderFrame>) {
    while let Some(frame) = frames.recv().await {
        let line = match frame::encode(&frame) {
            Ok(line) => line,
            Err(e) => {
                warn!("a reply didn't encode: {e}");
                continue;
            }
        };
        if writer.write_all(&line).await.is_err() || writer.flush().await.is_err() {
            return;
        }
    }
}

/// The provider, made at the first hello. A later one, as from a hub that lost its replies, gets the same answer.
fn hello(
    make: &mut Option<MakeProvider>,
    serving: &mut Option<(HubId, Arc<dyn Provider>)>,
    newest: u32,
    oldest: u32,
    hub_id: HubId,
) -> Result<Arc<dyn Provider>, ApiError> {
    if !(oldest..=newest).contains(&PROVIDER_PROTOCOL_VERSION) {
        return Err(ApiError::new(
            ErrorCode::VersionMismatch,
            format!(
                "this adapter speaks version {PROVIDER_PROTOCOL_VERSION} of the provider protocol, and the hub \
                 {oldest} to {newest}"
            ),
        ));
    }
    if let Some((serving_id, provider)) = serving {
        if *serving_id != hub_id {
            return Err(ApiError::new(ErrorCode::Conflict, format!("this adapter is serving hub {serving_id}")));
        }
        return Ok(provider.clone());
    }
    let make = make.take().ok_or_else(|| ApiError::new(ErrorCode::Internal, "the provider couldn't be made"))?;
    let provider = make(&hub_id).map_err(|e| e.to_api_error())?;
    *serving = Some((hub_id, provider.clone()));
    Ok(provider)
}

async fn answer(provider: &dyn Provider, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
    match request {
        ProviderRequest::Create { instance_id, from, size, display_name } => {
            provider.create(&instance_id, from.as_ref(), size, &display_name).await?;
        }
        ProviderRequest::Start { instance_id, workers } => provider.start(&instance_id, &workers).await?,
        ProviderRequest::Stop { instance_id } => provider.stop(&instance_id).await?,
        ProviderRequest::Kill { instance_id } => provider.kill(&instance_id).await?,
        ProviderRequest::Destroy { instance_id } => provider.destroy(&instance_id).await?,
        ProviderRequest::MakeBaseImage { instance_id } => provider.make_base_image(&instance_id).await?,
        ProviderRequest::List => return Ok(ProviderResponse::Instances { instances: provider.list().await? }),
        ProviderRequest::Connect { instance_id } => {
            return Ok(ProviderResponse::Command(provider.connect(&instance_id).await?));
        }
        // Answered before any request reaches here.
        ProviderRequest::Hello { .. } => return Err(ProviderError::Unsupported("say hello twice")),
        ProviderRequest::Unknown => return Err(ProviderError::Unsupported("answer a request from a newer hub")),
    }
    Ok(ProviderResponse::Ok)
}

#[cfg(test)]
mod tests;
