//! A made-up workspace and chat that the hints tests share.

use protocol::capability::calls::Call;
use protocol::capability::calls::call::Read;
use protocol::event::{ContentBlock, EventBody};
use protocol::fixtures;
use protocol::model::{Records, Repo, Rev, Seq};

use crate::records::{HubRecords, InWorkspace};
use crate::session_log::SessionLog;

pub(super) fn sample_workspace() -> InWorkspace {
    let mut records = HubRecords::default();
    let mut ws = fixtures::workspace("w1");
    ws.name = "entrecampos".to_string();
    ws.branch = "entrecampos/dictation-hints".into();
    ws.repo_id = "r1".into();
    let repo = Repo { name: "roundhouse".into(), rev: Rev::new(1), ..fixtures::repo("r1") };
    records.apply_list(Records { repos: vec![repo], workspaces: vec![ws], ..Records::default() });
    records.snapshot().in_workspace(&"w1".into()).unwrap()
}

pub(super) fn sample_log() -> SessionLog {
    let mut log = SessionLog::new(Seq::new(0));
    log.apply_event(fixtures::event(
        Seq::new(1),
        EventBody::Assistant {
            message_id: None,
            subagent: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: "Runs on Core ML and the Neural Engine via `cblas_sgemm`, unlike Whisper or `MAX_HINTS`.".into(),
                images: Vec::new(),
            }],
            first_index: None,
        },
    ));
    for seq in 2..45 {
        let read = Read { path: "crates/dictation/src/hints/words.rs".into(), lines: None, pages: None };
        let block = ContentBlock::Call { id: format!("t{seq}").into(), call: Call::Read(read) };
        log.apply_event(fixtures::event(
            Seq::new(seq),
            EventBody::Assistant {
                message_id: None,
                subagent: None,
                model: None,
                blocks: vec![block],
                first_index: None,
            },
        ));
    }
    log
}
