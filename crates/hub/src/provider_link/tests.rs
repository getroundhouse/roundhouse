use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::client::{ApiError, ErrorCode};
use providers::Feature;
use providers::local::{LocalConfig, LocalProvider};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

use super::*;

/// Opens an in-process adapter for each connection, numbered from 0, with `adapter` on its end of the stdio.
fn opening<F, Fut>(opens: Arc<AtomicUsize>, adapter: F) -> Open
where
    F: Fn(usize, DuplexStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    Box::new(move || {
        let (hub_end, adapter_end) = tokio::io::duplex(64 * 1024);
        tokio::spawn(adapter(opens.fetch_add(1, Ordering::SeqCst), adapter_end));
        let (reader, writer) = tokio::io::split(hub_end);
        Ok(Opened { reader: Box::new(reader), writer: Box::new(writer), process: None })
    })
}

fn local(dir: &Path) -> Arc<dyn Provider> {
    Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: dir.join("roundhoused"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        shell: Some("/bin/sh".to_string()),
        path: None,
    }))
}

/// The local provider's adapter, as `providers::serve` runs it.
fn serving_local(dir: &Path) -> Open {
    let made = local(dir);
    opening(Arc::default(), move |_, stdio| {
        let made = made.clone();
        async move {
            let (reader, writer) = tokio::io::split(stdio);
            let _ = providers::serve::serve(Box::new(move |_| Ok(made)), "test", reader, writer).await;
        }
    })
}

/// An adapter that answers each request in turn with the next of `answers`, and exits on reading one it has `None`
/// for. Once they're used up it stays, as an adapter does until its stdin closes. It writes what it's asked to
/// `asked`.
async fn scripted(stdio: DuplexStream, answers: Vec<Option<ProviderOutcome>>, asked: Arc<Mutex<Vec<Value>>>) {
    let (reader, mut writer) = tokio::io::split(stdio);
    let mut lines = BufReader::new(reader).lines();
    for answer in answers {
        let Ok(Some(line)) = lines.next_line().await else { return };
        let call: Value = serde_json::from_str(&line).unwrap();
        asked.lock().push(call["request"].clone());
        let Some(outcome) = answer else { return };
        let reply = ProviderFrame::Reply { id: call["id"].as_u64().unwrap(), outcome };
        writer.write_all(&frame::encode(&reply).unwrap()).await.unwrap();
    }
    while let Ok(Some(_)) = lines.next_line().await {}
}

fn hello() -> ProviderOutcome {
    ProviderOutcome::Response(ProviderResponse::Hello {
        protocol_version: PROVIDER_PROTOCOL_VERSION,
        adapter: "scripted".into(),
        build: "test".into(),
        capabilities: Capabilities { targets: vec![], features: vec![Feature::Snapshots] },
    })
}

fn ok() -> ProviderOutcome {
    ProviderOutcome::Response(ProviderResponse::Ok)
}

/// One that's said hello.
async fn started(open: Open) -> AdapterProvider {
    let provider = AdapterProvider::new("elsewhere".into(), "h".into(), open);
    provider.say_hello().await.unwrap();
    provider
}

#[tokio::test]
async fn it_answers_what_the_adapters_provider_answers() {
    let dir = tempfile::tempdir().unwrap();
    let provider = started(serving_local(dir.path())).await;
    let id = InstanceId::from("i");
    assert_eq!(provider.name(), "elsewhere");
    assert!(provider.capabilities().has(Feature::FilesystemSurvivesStop), "as the adapter's hello said");

    provider.create(&id, None, None, "roundhouse-i").await.unwrap();
    provider.start(&id, &[]).await.unwrap();
    let command = provider.connect(&id).await.unwrap();
    assert_eq!(command, local(dir.path()).connect(&id).await.unwrap(), "the hub runs what the provider says");
    let listed = provider.list().await.unwrap();
    assert_eq!(listed.iter().map(|i| i.instance_id.as_str()).collect::<Vec<_>>(), ["i"]);
    provider.stop(&id).await.unwrap();
    provider.destroy(&id).await.unwrap();

    let Err(ProviderError::Adapter(e)) = provider.connect(&id).await else { panic!("it's gone") };
    assert_eq!(e.code, ErrorCode::NotFound, "the adapter's code comes through: {e:?}");
    let Err(ProviderError::Adapter(e)) = provider.make_base_image(&id).await else { panic!("local makes none") };
    assert_eq!(e.code, ErrorCode::Unsupported);
}

#[tokio::test]
async fn a_request_whose_adapter_exits_before_answering_is_asked_again_of_a_new_one() {
    let opens = Arc::new(AtomicUsize::new(0));
    let asked: Arc<Mutex<Vec<Value>>> = Arc::default();
    let heard = asked.clone();
    let open = opening(opens.clone(), move |n, stdio| {
        let answer = (n > 0).then(ok);
        scripted(stdio, vec![Some(hello()), answer], heard.clone())
    });
    let provider = started(open).await;

    provider.start(&"i".into(), &[]).await.unwrap();

    assert_eq!(opens.load(Ordering::SeqCst), 2);
    let asked = asked.lock();
    let starts: Vec<_> = asked.iter().filter(|request| request["method"] == "start").collect();
    assert_eq!(starts.len(), 2, "{asked:?}");
    assert_eq!(starts[0], starts[1], "the same request again");
}

#[tokio::test]
async fn it_gives_up_on_an_adapter_that_exits_again() {
    let opens = Arc::new(AtomicUsize::new(0));
    let open = opening(opens.clone(), |_, stdio| scripted(stdio, vec![Some(hello()), None], Arc::default()));
    let provider = started(open).await;

    let Err(ProviderError::Io { source, .. }) = provider.stop(&"i".into()).await else { panic!("it can't answer") };
    assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(opens.load(Ordering::SeqCst), 2, "the first, and one it was asked again of");
}

#[tokio::test(start_paused = true)]
async fn an_adapter_that_doesnt_say_hello_within_ten_seconds_isnt_used() {
    let open = opening(Arc::default(), |_, stdio| async move {
        let _held = stdio;
        std::future::pending::<()>().await;
    });
    let provider = AdapterProvider::new("silent".into(), "h".into(), open);
    let asked = tokio::time::Instant::now();

    let said = tokio::time::timeout(Duration::from_secs(60), provider.say_hello()).await.expect("no time limit");
    let Err(ProviderError::Io { source, .. }) = said else { panic!("it never answered") };

    assert_eq!(source.kind(), io::ErrorKind::TimedOut);
    assert_eq!(asked.elapsed(), Duration::from_secs(10));
}

#[tokio::test]
async fn a_hello_that_the_adapter_refuses_fails_whatever_needed_it_as_io() {
    let refused = ProviderOutcome::Error(ApiError::new(ErrorCode::Conflict, "serving hub g"));
    let open = opening(Arc::default(), move |_, stdio| scripted(stdio, vec![Some(refused.clone())], Arc::default()));
    let provider = AdapterProvider::new("taken".into(), "h".into(), open);

    let stopped = provider.stop(&"i".into()).await;

    let Err(ProviderError::Io { source, .. }) = &stopped else { panic!("{stopped:?}") };
    assert_eq!(source.to_string(), "serving hub g");
}

#[tokio::test(start_paused = true)]
async fn a_request_an_adapter_never_answers_is_given_up_on() {
    let open = opening(Arc::default(), |_, stdio| scripted(stdio, vec![Some(hello())], Arc::default()));
    let provider = started(open).await;
    let asked = tokio::time::Instant::now();

    let started = tokio::time::timeout(REQUEST_TIMEOUT * 2, provider.start(&"i".into(), &[])).await;
    let Err(ProviderError::Io { source, .. }) = started.expect("no time limit") else { panic!("answered") };

    assert_eq!(source.kind(), io::ErrorKind::TimedOut);
    assert_eq!(asked.elapsed(), REQUEST_TIMEOUT);
}

/// A script that says hello as an adapter would, writes its PID to `pid`, and then runs `then`.
fn script(dir: &Path, then: &str) -> Open {
    let reply = frame::encode(&ProviderFrame::Reply { id: 1, outcome: hello() }).unwrap();
    let reply = String::from_utf8(reply).unwrap();
    let pid = dir.join("pid");
    let script = format!("echo $$ > {}; read -r hello; printf '%s' '{reply}'; {then}", pid.display());
    process(AdapterCommand { program: "/bin/sh".into(), args: vec!["-c".into(), script] })
}

fn pid(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("pid")).unwrap().trim().parse().unwrap()
}

#[tokio::test]
async fn shutting_down_waits_for_an_adapter_that_exits_once_its_stdin_closes() {
    let dir = tempfile::tempdir().unwrap();
    let finished = dir.path().join("finished");
    let provider = started(script(dir.path(), &format!("cat > /dev/null; : > {}", finished.display()))).await;
    let asked = std::time::Instant::now();

    provider.shut_down(ADAPTER_GRACE).await;

    assert!(finished.exists(), "it should have finished by itself");
    assert!(asked.elapsed() < ADAPTER_GRACE / 2);
    assert!(!process::testing::is_running(pid(dir.path())));
}

#[tokio::test]
async fn shutting_down_ends_an_adapter_that_ignores_its_stdin_closing() {
    let dir = tempfile::tempdir().unwrap();
    let provider = started(script(dir.path(), "while :; do sleep 0.05; done")).await;
    let pid = pid(dir.path());
    assert!(process::testing::is_running(pid), "the adapter is running");

    provider.shut_down(Duration::from_millis(100)).await;

    assert!(!process::testing::is_running(pid), "it should have been ended");
}

#[tokio::test]
async fn an_adapter_that_exits_leaving_its_stdout_open_isnt_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let helpers = dir.path().join("helpers");
    // Exits on the request after hello, leaving something it started holding its stdout, in its process group.
    let then = format!("read -r request; sleep 100 & echo $$ >> {}; exit", helpers.display());
    let provider = started(script(dir.path(), &then)).await;

    let stopped = tokio::time::timeout(Duration::from_secs(10), provider.stop(&"i".into())).await;

    let Ok(Err(ProviderError::Io { source, .. })) = stopped else { panic!("{stopped:?}") };
    assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
    for group in std::fs::read_to_string(&helpers).unwrap().lines() {
        process::signal_group(group.parse().unwrap(), process::Signal::Kill);
    }
}

#[tokio::test]
async fn shutting_down_gives_up_on_a_hello_in_flight_and_starts_no_more_adapters() {
    let opens = Arc::new(AtomicUsize::new(0));
    let open = opening(opens.clone(), |_, stdio| scripted(stdio, vec![], Arc::default()));
    let provider = Arc::new(AdapterProvider::new("slow".into(), "h".into(), open));
    let hello = tokio::spawn({
        let provider = provider.clone();
        async move { provider.say_hello().await }
    });
    while opens.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let asked = std::time::Instant::now();

    provider.shut_down(ADAPTER_GRACE).await;

    assert!(asked.elapsed() < Duration::from_secs(1), "it waited for the hello");
    let Err(ProviderError::Io { source, .. }) = hello.await.unwrap() else { panic!("it said hello") };
    assert_eq!(source.kind(), io::ErrorKind::Interrupted);
    assert!(provider.stop(&"i".into()).await.is_err());
    assert_eq!(opens.load(Ordering::SeqCst), 1, "nothing started it again");
}
