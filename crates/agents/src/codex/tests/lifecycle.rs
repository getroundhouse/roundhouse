#![cfg(test)]
use super::harness::Harness;
use crate::AgentOutput;
use serde_json::json;

#[tokio::test]
async fn abandoning_initialization_kills_the_process_group_and_removes_its_record() {
    let mut h = Harness::new(&json!([{"on":"initialize","actions":[
        "descendant",{"method":"test/ready","params":{}}
    ]}]));
    loop {
        if let AgentOutput::Event { body: protocol::event::EventBody::AgentOther { kind, .. }, .. } = h.next().await
            && kind == "test/ready"
        {
            break;
        }
    }
    let pid_path = h.room.path().join("pid");
    let pid = process::ProcessRecord::read(&pid_path).unwrap();
    let child: u32 = std::fs::read_to_string(h.room.path().join("descendant")).unwrap().trim().parse().unwrap();
    assert!(pid.is_running());
    assert!(process::start_time(child).is_some());
    h.launch.abort();
    let _ = (&mut h.launch).await;
    loop {
        if matches!(h.next().await, AgentOutput::Exited { .. }) {
            break;
        }
    }
    assert!(!pid.is_running());
    assert!(process::start_time(child).is_none());
    assert!(!pid_path.exists());
}

#[tokio::test]
async fn cancelling_an_account_call_stops_its_native_process() {
    use crate::{AgentOptions, AgentProcess, BinaryLookup};
    let mut h = Harness::new(&json!([{"on":"account/read","actions":["account-ready"]}]));
    h.started().await.stop().await;
    let room = h.room.path().to_owned();
    let pid_path = room.join("account-pid");
    let options = AgentOptions {
        binary: BinaryLookup { explicit: Some(super::program::program()), ..Default::default() },
        pid_file: Some(pid_path.clone()),
        ..Default::default()
    };
    let call_room = room.clone();
    let call = tokio::spawn(async move { crate::codex::account(&options, &call_room).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !room.join("account-ready").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let record = process::ProcessRecord::read(&pid_path).unwrap();
    assert!(record.is_running());
    call.abort();
    let _ = call.await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while pid_path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!record.is_running());
}
