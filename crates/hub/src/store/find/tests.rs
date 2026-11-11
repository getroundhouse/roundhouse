use protocol::event::{ContentBlock, EventBody};
use protocol::find::{Found, Needle};
use protocol::model::{Place, ToolUseId};

use crate::store::tests::{notice, store_with_session};

fn prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { text: text.to_string(), client_msg_id: None, attachments: Vec::new() }
}

/// What the subagent `call` started says.
fn subagent_says(call: &str, text: &str) -> EventBody {
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: Some(call.into()),
        model: None,
        blocks: vec![ContentBlock::Text { text: text.into(), images: Vec::new() }],
        first_index: None,
    }
}

fn found(places: &[u64], through: u64) -> Found {
    Found { places: places.iter().copied().map(Place::new).collect(), through: Place::new(through) }
}

#[test]
fn the_events_that_hold_the_text_are_found_by_place_oldest_first_as_of_the_newest_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let append = |body: &EventBody| store.append(&"s".into(), None, body, None, None, None).unwrap();
    append(&prompt("Make the train slow down"));
    append(&subagent_says("call", "the train's timetable"));
    append(&EventBody::PromptTaken { prompt: 1 });
    append(&notice("the TRAIN is late"));
    append(&subagent_says("call", "nothing here"));
    append(&notice("nothing here"));
    let needle = Needle::new("train").unwrap();
    let call: ToolUseId = "call".into();

    assert_eq!(store.find_events(&"s".into(), None, &needle).unwrap(), found(&[1, 3], 4), "the agent's own");
    assert_eq!(store.find_events(&"s".into(), Some(&call), &needle).unwrap(), found(&[1], 2), "the subagent's");
    let none = found(&[], 0);
    assert_eq!(store.find_events(&"other".into(), None, &needle).unwrap(), none, "a chat with no events has none");
}

/// The find doesn't share the connection reads use, so neither waits for the other.
#[test]
fn a_chat_is_looked_through_on_a_connection_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(store_with_session(dir.path()));
    store.append(&"s".into(), None, &prompt("the train"), None, None, None).unwrap();
    // As a long read would.
    let reading = super::lock(&store.reader);

    let finding = std::thread::spawn({
        let store = store.clone();
        move || store.find_events(&"s".into(), None, &Needle::new("train").unwrap()).unwrap().places
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !finding.is_finished() {
        assert!(std::time::Instant::now() < deadline, "the find waited for a read");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(reading);
    assert_eq!(finding.join().unwrap(), [Place::new(1)]);
}

#[test]
fn text_written_another_way_in_json_is_still_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.append(&"s".into(), None, &prompt("say \"slow\"\nthen stop"), None, None, None).unwrap();

    let found = |query| store.find_events(&"s".into(), None, &Needle::new(query).unwrap()).unwrap().places;

    assert_eq!(found("\"slow\""), [Place::new(1)], "a quote, which JSON writes with a backslash");
    let only_in_json = "\":\"say";
    assert!(serde_json::to_string(&prompt("say")).unwrap().contains(only_in_json));
    assert!(found(only_in_json).is_empty(), "and nothing that's only in how JSON writes it");
}
