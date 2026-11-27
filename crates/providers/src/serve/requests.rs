use crate::{Provider, ProviderError};
use protocol::client::{ApiError, ErrorCode};
use protocol::frame;
use protocol::frame::Call;
use protocol::model::HubId;
use protocol::provider::{
    ANSWERED_WITHIN, Create, Destroy, EndWorker, Hello, HelloReply, Instances, KillWorker, MakeBaseImage,
    PROVIDER_PROTOCOL_VERSION, ProviderFrame, ProviderOutcome, ProviderRequest, ProviderResponse, Start,
};
use protocol::versions::Versions;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::warn;

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
                warn!(
                    name: "provider.request_unread",
                    kind = ?e.classify(),
                    column = e.column(),
                    "Ignoring a request that didn't parse"
                );
                continue;
            }
        };
        let reply = move |outcome| ProviderFrame::Reply { id, outcome };
        match (request, &serving) {
            (Ok(ProviderRequest::Hello(Hello { protocol_version, oldest_protocol_version, hub_id, .. })), _) => {
                let theirs = Versions::from_hello(protocol_version, Some(oldest_protocol_version));
                let outcome = hello_outcome(&mut make, &mut serving, theirs, hub_id, build).await;
                let _ = frames.send(reply(outcome)).await;
            }
            (_, None) => {
                let refused = ApiError::new(ErrorCode::VersionMismatch, "the first request must be hello");
                let _ = frames.send(reply(ProviderOutcome::Error(refused))).await;
            }
            // A newer hub's, so it's refused as one this adapter doesn't do, rather than leaving the hub waiting.
            (Err(e), Some(_)) => {
                let refused = ApiError::unsupported_request("this adapter", build, Some(&e));
                let _ = frames.send(reply(ProviderOutcome::Error(refused))).await;
            }
            (Ok(request), Some((_, provider))) => answer_on_a_task(provider.clone(), request, id, frames.clone()),
        }
    }
    writing.abort();
    Ok(())
}

/// How long a provider gets to answer a request before it's answered as having failed: half a minute short of how long
/// the hub waits, so the hub hears why, rather than only that the adapter took too long. A hub that's starting gives a
/// few of them less, as `ANSWERED_AS_THE_HUB_STARTS_WITHIN` says, which a provider meets by itself.
const GIVES_UP_AFTER: Duration = Duration::from_secs(ANSWERED_WITHIN.as_secs() - 30);

/// So a slow request, such as starting a VM, doesn't hold up the rest, and one whose answer panics or never comes is
/// still answered. Giving up drops the provider's future, as the [`Provider`] trait says.
fn answer_on_a_task(
    provider: Arc<dyn Provider>,
    request: ProviderRequest,
    id: u64,
    frames: mpsc::Sender<ProviderFrame>,
) {
    tokio::spawn(async move {
        let answering =
            tokio::spawn(async move { tokio::time::timeout(GIVES_UP_AFTER, answer(provider.as_ref(), request)).await });
        let outcome = match answering.await {
            Ok(Ok(Ok(response))) => ProviderOutcome::Response(response),
            Ok(Ok(Err(e))) => ProviderOutcome::Error(e.to_api_error()),
            Ok(Err(_)) => ProviderOutcome::Error(ApiError::new(
                ErrorCode::Worker,
                format!("the provider didn't finish within {GIVES_UP_AFTER:?}, so the adapter gave up on it"),
            )),
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
                warn!(name: "provider.reply_unencoded", error = %e, "A reply didn't encode");
                continue;
            }
        };
        if writer.write_all(&line).await.is_err() || writer.flush().await.is_err() {
            return;
        }
    }
}

async fn hello_outcome(
    make: &mut Option<MakeProvider>,
    serving: &mut Option<(HubId, Arc<dyn Provider>)>,
    theirs: Versions,
    hub_id: HubId,
    build: &str,
) -> ProviderOutcome {
    let (provider, chosen) = match hello(make, serving, theirs, hub_id) {
        Ok(ready) => ready,
        Err(e) => return ProviderOutcome::Error(e),
    };
    let environment = match provider.environment().await {
        Ok(environment) => environment,
        Err(e) => return ProviderOutcome::Error(e.to_api_error()),
    };
    ProviderOutcome::Response(ProviderResponse::Hello(HelloReply {
        protocol_version: chosen,
        adapter: provider.name().to_string(),
        build: build.to_string(),
        capabilities: provider.capabilities(),
        environment,
    }))
}

/// The provider, made at the first hello, and the version chosen. A later one, as from a hub that lost its replies, gets
/// the same answer. An adapter speaks only its own version.
fn hello(
    make: &mut Option<MakeProvider>,
    serving: &mut Option<(HubId, Arc<dyn Provider>)>,
    theirs: Versions,
    hub_id: HubId,
) -> Result<(Arc<dyn Provider>, u32), ApiError> {
    let ours = Versions::only(PROVIDER_PROTOCOL_VERSION);
    let Some(chosen) = ours.newest_shared(theirs) else {
        let message = format!("this adapter speaks {ours} of the provider protocol, and the hub {theirs}");
        return Err(ApiError::new(ErrorCode::VersionMismatch, message));
    };
    if let Some((serving_id, provider)) = serving {
        if *serving_id != hub_id {
            return Err(ApiError::new(ErrorCode::Conflict, format!("this adapter is serving hub {serving_id}")));
        }
        return Ok((provider.clone(), chosen));
    }
    let make = make.take().ok_or_else(|| ApiError::new(ErrorCode::Internal, "the provider couldn't be made"))?;
    let provider = make(&hub_id).map_err(|e| e.to_api_error())?;
    *serving = Some((hub_id, provider.clone()));
    Ok((provider, chosen))
}

async fn answer(provider: &dyn Provider, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
    match request {
        ProviderRequest::Create(Create { instance_id, from, size }) => {
            provider.create(&instance_id, from.as_ref(), size).await?;
        }
        ProviderRequest::Start(Start { instance_id, workers, ticket }) => {
            provider.start(&instance_id, &workers, Some(&ticket)).await?;
        }
        ProviderRequest::EndWorker(EndWorker { instance_id }) => provider.end_worker(&instance_id).await?,
        ProviderRequest::KillWorker(KillWorker { instance_id }) => provider.kill_worker(&instance_id).await?,
        ProviderRequest::Destroy(Destroy { instance_id }) => provider.destroy(&instance_id).await?,
        ProviderRequest::MakeBaseImage(MakeBaseImage { instance_id }) => provider.make_base_image(&instance_id).await?,
        ProviderRequest::List => {
            return Ok(ProviderResponse::Instances(Instances { instances: provider.list().await? }));
        }
        ProviderRequest::OpenPreflight(launch) => {
            return Ok(ProviderResponse::Command(provider.open_preflight(&launch).await?));
        }
        // Answered before any request reaches here.
        ProviderRequest::Hello(_) => return Err(ProviderError::Unsupported("say hello twice")),
        ProviderRequest::Unknown => return Err(ProviderError::Unsupported("answer a request from a newer hub")),
    }
    Ok(ProviderResponse::Ok)
}
