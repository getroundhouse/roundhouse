use protocol::event::EventBody;
use protocol::model::{Seq, SessionId};
use protocol::prompt::Prompt;

use super::*;
use crate::store::tests::store_with_session;

fn prompt() -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("x") }
}

fn append(store: &Store, body: &EventBody) -> Option<Vec<Seq>> {
    store.append(&"s".into(), None, body, None, None, None).unwrap().untaken
}

#[test]
fn a_prompt_waits_on_the_record_until_its_taken_or_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert_eq!(append(&store, &prompt()), Some(vec![1]));
    assert_eq!(append(&store, &EventBody::Unknown), None);
    assert_eq!(append(&store, &prompt()), Some(vec![1, 3]));
    assert_eq!(store.untaken_prompts().unwrap(), [(SessionId::from("s"), vec![1, 3])]);

    assert_eq!(append(&store, &EventBody::PromptTaken { prompt: 1 }), Some(vec![3]));
    assert_eq!(append(&store, &EventBody::PromptDropped { prompt: 3 }), Some(Vec::new()));
    assert_eq!(append(&store, &EventBody::PromptTaken { prompt: 3 }), None, "settled already");
    assert!(store.untaken_prompts().unwrap().is_empty());
}

/// So a hub that went away before the agent took it says it was dropped, as for a prompt.
#[test]
fn a_compaction_asked_for_waits_on_the_record_too() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert_eq!(append(&store, &EventBody::CompactionAsked { keeping: None }), Some(vec![1]));
    assert_eq!(store.untaken_prompts().unwrap(), [(SessionId::from("s"), vec![1])]);
    assert_eq!(append(&store, &EventBody::PromptTaken { prompt: 1 }), Some(Vec::new()));
}
