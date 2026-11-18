//! What a worker of protocol 4, the version before this one, sends that this one has no form for, spelled out, since a
//! worker of an older build carries on with its agents after its hub updates, until its workspace is idle.

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::frame;
use protocol::model::{CacheRequest, CacheTtl, ContextUsage, ModelLabel, ModelName, RunId};
use protocol::worker::WorkerFrame;
use protocol::worker::previous::Frame;

/// `line`, as a worker of protocol 4 writes it, as the frame this protocol would send.
fn read(line: &str) -> WorkerFrame {
    WorkerFrame::from(frame::decode::<Frame>(line.as_bytes()).unwrap())
}

fn reported(report: Report, seq: Option<u64>) -> WorkerFrame {
    WorkerFrame::AgentReport { session_id: "s".into(), run: RunId::new(4), report, seq }
}

/// How full the context is, and a request's cache, each as its report, numbered for a hub that asked the worker to
/// keep running without it, and not otherwise.
#[test]
fn what_a_worker_of_protocol_4_says_of_the_context_and_the_cache_reads_as_a_report() {
    let context = read(
        r#"{"frame":"agent_context","session_id":"s","run":4,"context":{"used_tokens":5,"max_tokens":9,"parts":[]},"seq":2}"#,
    );
    let usage = ContextUsage { used_tokens: 5, max_tokens: 9, model_tokens: None, max_set_by: None, parts: vec![] };
    assert_eq!(context, reported(Report::Context { context: usage }, Some(2)));

    let cache = read(
        r#"{"frame":"agent_cache","session_id":"s","run":4,"request":{"sent_ms_ago":40,"ttl":"one_hour","read_tokens":7,"written_tokens":3,"model":"opus","first_for_prompt":true}}"#,
    );
    let request = CacheRequest {
        sent_ms_ago: 40,
        ttl: Some(CacheTtl::OneHour),
        read_tokens: 7,
        written_tokens: 3,
        model: Some(ModelName::named("opus")),
        first_for_prompt: true,
    };
    assert_eq!(cache, reported(Report::Cache { request }, None));
}

/// The account's limits, each by the window it counts over, and whether they're every model's own week, which a worker
/// from before that leaves out.
#[test]
fn what_a_worker_of_protocol_4_says_of_the_limits_reads_as_a_report() {
    let limits = read(
        r#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[{"window":"five_hours","used_percent":12,"resets_at_ms":5,"heard_at_ms":null},{"window":"model_seven_days","model":"Fable","used_percent":88,"resets_at_ms":9,"heard_at_ms":null}],"every_models_week":true,"seq":3}"#,
    );
    let limit = |window_minutes, model: Option<&str>, used_percent, resets_at_ms| Limit {
        window_minutes: Some(window_minutes),
        model: model.map(ModelLabel::new),
        used_percent,
        resets_at_ms: Some(resets_at_ms),
        name: None,
    };
    let said = vec![limit(300, None, 12, 5), limit(10_080, Some("Fable"), 88, 9)];
    assert_eq!(limits, reported(Report::Limits { limits: said, every_models_week: true }, Some(3)));

    let unsaid = read(r#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[]}"#);
    assert_eq!(unsaid, reported(Report::Limits { limits: vec![], every_models_week: false }, None));
}
