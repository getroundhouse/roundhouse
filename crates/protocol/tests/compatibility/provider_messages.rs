//! Every provider message, spelled out, because an adapter can be written in any language, against what goes over
//! the wire rather than against these types, and how a newer side's messages read, since adapters don't ship with
//! the app and can be older or newer than their hub.

use protocol::client::{ApiError, ErrorCode};
use protocol::frame;
use protocol::provider::{
    Capabilities, ClonePersistence, Command, Create, Destroy, EndWorker, Feature, Hello, HelloReply, Instance,
    InstanceState, Instances, KillWorker, MakeBaseImage, OpenPreflight, ProviderCall, ProviderEvent, ProviderFrame,
    ProviderOutcome, ProviderRequest, ProviderResponse, Size, Start, WorkerBinary, WorkerEnvironment, WorkerFilesystem,
};

use super::keeps_its_form;

fn call(request: ProviderRequest) -> ProviderCall {
    ProviderCall { id: 7, request }
}

#[test]
fn every_request_keeps_its_form() {
    keeps_its_form(
        &call(ProviderRequest::Create(Create {
            instance_id: "i".into(),
            from: Some("base".into()),
            size: Some(Size { cpus: 4, memory_mib: 8192 }),
        })),
        r#"{"id":7,"request":{"method":"create","instance_id":"i","from":"base","size":{"cpus":4,"memory_mib":8192}}}"#,
    );
    keeps_its_form(
        &call(ProviderRequest::Start(Start {
            instance_id: "i".into(),
            workers: vec![WorkerBinary {
                target: "x86_64-unknown-linux-gnu".into(),
                path: "/opt/roundhouse/worker".into(),
                sha256: "ab12".into(),
            }],
            ticket: "eyJ0aWNrZXQiOiJpdCJ9".to_string().into(),
        })),
        r#"{"id":7,"request":{"method":"start","instance_id":"i","workers":[{"target":"x86_64-unknown-linux-gnu","path":"/opt/roundhouse/worker","sha256":"ab12"}],"ticket":"eyJ0aWNrZXQiOiJpdCJ9"}}"#,
    );
    keeps_its_form(
        &call(ProviderRequest::EndWorker(EndWorker { instance_id: "i".into() })),
        r#"{"id":7,"request":{"method":"end_worker","instance_id":"i"}}"#,
    );
    keeps_its_form(
        &call(ProviderRequest::KillWorker(KillWorker { instance_id: "i".into() })),
        r#"{"id":7,"request":{"method":"kill_worker","instance_id":"i"}}"#,
    );
    keeps_its_form(
        &call(ProviderRequest::Destroy(Destroy { instance_id: "i".into() })),
        r#"{"id":7,"request":{"method":"destroy","instance_id":"i"}}"#,
    );
    keeps_its_form(&call(ProviderRequest::List), r#"{"id":7,"request":{"method":"list"}}"#);
    keeps_its_form(
        &call(ProviderRequest::OpenPreflight(OpenPreflight {
            project: protocol::model::RepoOrigin::Checkout { path: "/repo".into() },
            agent: protocol::model::AgentKind::Codex,
        })),
        r#"{"id":7,"request":{"method":"open_preflight","project":{"kind":"checkout","path":"/repo"},"agent":"codex"}}"#,
    );
    keeps_its_form(
        &call(ProviderRequest::MakeBaseImage(MakeBaseImage { instance_id: "i".into() })),
        r#"{"id":7,"request":{"method":"make_base_image","instance_id":"i"}}"#,
    );
}

fn reply(response: ProviderResponse) -> ProviderFrame {
    ProviderFrame::Reply { id: 7, outcome: ProviderOutcome::Response(response) }
}

#[test]
fn every_response_and_event_keeps_its_form() {
    keeps_its_form(&reply(ProviderResponse::Ok), r#"{"frame":"reply","id":7,"response":{"result":"ok"}}"#);
    keeps_its_form(
        &ProviderFrame::Reply { id: 7, outcome: ProviderOutcome::Error(ApiError::new(ErrorCode::NotFound, "no i")) },
        r#"{"frame":"reply","id":7,"error":{"code":"not_found","message":"no i"}}"#,
    );
    keeps_its_form(
        &reply(ProviderResponse::Instances(Instances {
            instances: vec![
                Instance { instance_id: "i".into(), state: InstanceState::Running, base_image: false },
                Instance { instance_id: "base".into(), state: InstanceState::Stopped, base_image: true },
            ],
        })),
        r#"{"frame":"reply","id":7,"response":{"result":"instances","instances":[{"instance_id":"i","state":"running","base_image":false},{"instance_id":"base","state":"stopped","base_image":true}]}}"#,
    );
    keeps_its_form(
        &ProviderFrame::Event {
            event: ProviderEvent::InstanceEnding {
                instance_id: "i".into(),
                at_ms: Some(1_700_000_000_000),
                reason: "the cloud is taking back a spot instance".into(),
            },
        },
        r#"{"frame":"event","event":{"kind":"instance_ending","instance_id":"i","at_ms":1700000000000,"reason":"the cloud is taking back a spot instance"}}"#,
    );
    keeps_its_form(
        &ProviderFrame::Event {
            event: ProviderEvent::InstanceLost { instance_id: "i".into(), reason: "its host is gone".into() },
        },
        r#"{"frame":"event","event":{"kind":"instance_lost","instance_id":"i","reason":"its host is gone"}}"#,
    );
}

#[test]
fn every_instance_state_keeps_its_form() {
    let states = [
        (InstanceState::Stopped, "stopped"),
        (InstanceState::Starting, "starting"),
        (InstanceState::Running, "running"),
        (InstanceState::Stopping, "stopping"),
    ];
    for (state, name) in states {
        keeps_its_form(&state, &format!("\"{name}\""));
    }
}

/// Spelled out, because a hub and an adapter that differ by a break find a version they share through these.
#[test]
fn the_provider_hellos_keep_their_form() {
    let hello = ProviderCall {
        id: 1,
        request: ProviderRequest::Hello(Hello {
            protocol_version: 2,
            oldest_protocol_version: 1,
            hub_id: "h".into(),
            build: "b".into(),
        }),
    };
    assert_eq!(
        frame::encode(&hello).unwrap(),
        b"{\"id\":1,\"request\":{\"method\":\"hello\",\"protocol_version\":2,\"oldest_protocol_version\":1,\"hub_id\":\"h\",\"build\":\"b\"}}\n"
    );

    let reply: ProviderFrame = frame::decode(
        br#"{"frame":"reply","id":1,"response":{"result":"hello","protocol_version":1,"adapter":"local","build":"b","capabilities":{"targets":["aarch64-apple-darwin"],"features":["filesystem_survives_stop"]}}}"#,
    )
    .unwrap();

    let ProviderFrame::Reply {
        outcome:
            ProviderOutcome::Response(ProviderResponse::Hello(HelloReply {
                protocol_version,
                capabilities,
                environment,
                ..
            })),
        ..
    } = reply
    else {
        panic!("{reply:?}");
    };
    assert_eq!(protocol_version, 1);
    assert_eq!(capabilities.targets, ["aarch64-apple-darwin".into()]);
    assert!(capabilities.has(Feature::FilesystemSurvivesStop) && !capabilities.has(Feature::BaseImages));
    assert_eq!(environment, WorkerEnvironment::default(), "omitted environment defaults to separate ephemeral compute");
}

/// An adapter written in Rust answers a hello in the form a hub reads.
#[test]
fn an_adapters_hello_is_written_as_a_hub_reads_it() {
    let hello = ProviderResponse::Hello(HelloReply {
        protocol_version: 1,
        adapter: "local".into(),
        build: "b".into(),
        capabilities: Capabilities {
            targets: vec!["aarch64-apple-darwin".into()],
            features: vec![Feature::FilesystemSurvivesStop],
        },
        environment: WorkerEnvironment::default(),
    });

    keeps_its_form(
        &reply(hello),
        r#"{"frame":"reply","id":7,"response":{"result":"hello","protocol_version":1,"adapter":"local","build":"b","capabilities":{"targets":["aarch64-apple-darwin"],"features":["filesystem_survives_stop"]}}}"#,
    );

    let on_hub = ProviderResponse::Hello(HelloReply {
        protocol_version: 1,
        adapter: "local".into(),
        build: "b".into(),
        capabilities: Capabilities::default(),
        environment: WorkerEnvironment::HUB_MACHINE,
    });
    keeps_its_form(
        &reply(on_hub),
        r#"{"frame":"reply","id":7,"response":{"result":"hello","protocol_version":1,"adapter":"local","build":"b","capabilities":{"targets":[],"features":[]},"environment":{"filesystem":"hub","clones":"persistent"}}}"#,
    );
}

/// A request with no fields reads with fields a newer hub added to it, as any request does.
#[test]
fn a_list_reads_with_fields_it_doesnt_know() {
    let call: ProviderCall = frame::decode(br#"{"id":4,"request":{"method":"list","newer":1}}"#).unwrap();

    assert_eq!(call, ProviderCall { id: 4, request: ProviderRequest::List });
}

/// What an adapter leaves out of its capabilities it can't do, and an instance it lists without saying it's a base
/// image isn't one.
#[test]
fn what_an_adapter_leaves_out_it_doesnt_have() {
    let capabilities: Capabilities = frame::decode(b"{}").unwrap();
    let instance: Instance = frame::decode(br#"{"instance_id":"i","state":"running"}"#).unwrap();

    assert_eq!(capabilities, Capabilities::default());
    assert!(!instance.base_image);
}

#[test]
fn a_provider_request_from_a_newer_hub_still_decodes_with_its_id() {
    let line = br#"{"id":4,"request":{"method":"from_a_newer_hub","instance_id":"i","count":2}}"#;

    let call: ProviderCall = frame::decode(line).unwrap();

    assert_eq!(call, ProviderCall { id: 4, request: ProviderRequest::Unknown });
}

#[test]
fn an_event_state_or_feature_from_a_newer_adapter_reads_as_unknown() {
    let event: ProviderFrame =
        frame::decode(br#"{"frame":"event","event":{"kind":"from_a_newer_adapter","instance_id":"i"}}"#).unwrap();
    let state: InstanceState = frame::decode(br#""hibernating""#).unwrap();
    let capabilities: Capabilities = frame::decode(br#"{"features":["teleports"]}"#).unwrap();
    let environment: WorkerEnvironment = frame::decode(br#"{"filesystem":"mounted","clones":"snapshotted"}"#).unwrap();

    assert_eq!(event, ProviderFrame::Event { event: ProviderEvent::Unknown });
    assert_eq!(state, InstanceState::Unknown);
    assert_eq!(capabilities.features, [Feature::Unknown]);
    assert_eq!(
        environment,
        WorkerEnvironment { filesystem: WorkerFilesystem::Unknown, clones: ClonePersistence::Unknown }
    );
}

#[test]
fn a_create_leaves_out_what_it_doesnt_set() {
    let create = ProviderCall {
        id: 2,
        request: ProviderRequest::Create(Create { instance_id: "i".into(), from: None, size: None }),
    };

    assert_eq!(
        frame::encode(&create).unwrap(),
        b"{\"id\":2,\"request\":{\"method\":\"create\",\"instance_id\":\"i\"}}\n"
    );
}

#[test]
fn a_preflight_answers_with_a_command_and_an_adapter_refuses_with_an_api_error() {
    let command: ProviderFrame = frame::decode(
        br#"{"frame":"reply","id":5,"response":{"result":"command","program":"incus","args":["exec","i"],"env":[["A","b"]]}}"#,
    )
    .unwrap();
    let refused: ProviderFrame =
        frame::decode(br#"{"frame":"reply","id":6,"error":{"code":"unsupported","message":"no"}}"#).unwrap();

    let sent =
        Command { program: "incus".into(), args: vec!["exec".into(), "i".into()], env: vec![("A".into(), "b".into())] };
    assert_eq!(
        command,
        ProviderFrame::Reply { id: 5, outcome: ProviderOutcome::Response(ProviderResponse::Command(sent)) }
    );
    let error = ApiError::new(ErrorCode::Unsupported, "no");
    assert_eq!(refused, ProviderFrame::Reply { id: 6, outcome: ProviderOutcome::Error(error) });
}
