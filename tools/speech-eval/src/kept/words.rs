//! The words each recording is heard with: those it was heard with, none, or those today's picker gives, from what its
//! own were picked from and the chat it was in, which the hub has.

use std::path::Path;

use anyhow::Context as _;
use client_core::hints;
use client_core::session_log::{MAX_LOADED_EVENTS, SessionLog};
use dictation::{Hint, Hints};
use hint_words::{HintWord, PickedFrom};
use hub::store::{Places, Store};
use kept_recordings::Kept;
use protocol::model::Place;
use tempfile::TempDir;

use super::hearing::Settings;
use super::recordings::Recording;

/// The recordings to hear, each with its words.
pub(super) struct ToHear {
    pub(super) each: Vec<(Recording, Hints)>,
    /// How many have the words they were heard with.
    pub(super) same: usize,
    /// How many aren't heard, since they don't say what their words were picked from, or the hub no longer has all of
    /// the chat they were.
    pub(super) gone: usize,
}

impl ToHear {
    /// `recordings` with the words `settings` say, where today's are picked from what's in `chats`.
    pub(super) fn of(recordings: Vec<Recording>, settings: Settings, chats: Option<&Chats>) -> anyhow::Result<Self> {
        let mut to_hear = Self { each: Vec::new(), same: 0, gone: 0 };
        for recording in recordings {
            let kept = &recording.kept;
            let was = Hints::from_kept(kept.hints.iter().map(hint));
            let words = match chats {
                _ if settings.no_hints => Some(Hints::default()),
                Some(chats) => chats.todays_words(kept)?,
                None => Some(was.clone()),
            };
            let Some(words) = words else {
                to_hear.gone += 1;
                continue;
            };
            to_hear.same += usize::from(words == was);
            to_hear.each.push((recording, words));
        }
        Ok(to_hear)
    }
}

/// The hub's chats, in a copy of its database, since this build may have to migrate it to read it, and a hub may be
/// writing it.
pub(crate) struct Chats {
    store: Store,
    /// Where the copy is, which goes with this.
    _copy: TempDir,
}

impl Chats {
    /// Those in the database at `hub`, copied into a folder of its own in `beside` until this is dropped.
    pub(crate) fn open(hub: &Path, beside: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(beside).with_context(|| format!("couldn't make {}", beside.display()))?;
        let dir = TempDir::new_in(beside)?;
        let copy = dir.path().join("hub.db");
        hub::store::copy(hub, &copy).with_context(|| format!("couldn't copy {}", hub.display()))?;
        let store = Store::open(&copy, &dir.path().join("backups"))
            .with_context(|| format!("couldn't read a copy of {}", hub.display()))?;
        Ok(Self { store, _copy: dir })
    }

    /// What the recording `kept` would be heard with today, or `None` if that's gone, as [`ToHear::gone`] says.
    fn todays_words(&self, kept: &Kept) -> anyhow::Result<Option<Hints>> {
        let Some(from) = &kept.picked_from else { return Ok(None) };
        let Some(chat) = self.chat(kept.chat.as_deref(), from)? else { return Ok(None) };
        Ok(Some(Hints::new(hints::pick(from, Some(&chat)).iter().map(hint))))
    }

    /// As much of the chat as the app had loaded, or `None` if any of that's gone.
    fn chat(&self, id: Option<&str>, from: &PickedFrom) -> anyhow::Result<Option<SessionLog>> {
        let mut read = Vec::new();
        for loaded in &from.chat {
            let Some(id) = id else { return Ok(None) };
            let count = u32::try_from((loaded.last + 1).saturating_sub(loaded.first))?;
            let places = Places { subagent: None, from: Place::new(loaded.first), through: None };
            let (events, _last) = self.store.read_places(&id.into(), places, count, u64::MAX, false)?;
            if events.len() != count as usize {
                return Ok(None);
            }
            read.extend(events);
        }
        // A log lets go of those furthest from its first row past this many, which would be the newest, and words are
        // picked from the newest.
        let older = read.len().saturating_sub(MAX_LOADED_EVENTS);
        let mut log = SessionLog::new(Place::default());
        for event in read.drain(older..) {
            log.apply_event(event);
        }
        Ok(Some(log))
    }
}

/// `word` as the engine takes it.
fn hint(word: &HintWord) -> Hint {
    Hint { text: word.text.clone(), spelled: word.spelled }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use hint_words::{PickedFrom, Places as Loaded};
    use hub::store::{Store, StoredSession, WorkspacePlacement};
    use protocol::event::{ContentBlock, EventBody};
    use protocol::fixtures;
    use protocol::prompt::{Prompt, PromptFrom};

    use super::super::recordings::Recording;
    use super::super::tests::kept;
    use super::{Chats, Hints, Kept, Settings, ToHear};

    const TODAYS: Settings = Settings { boost: None, no_hints: false, todays_words: true };

    /// The hub's database at `path`, whose session `s` has a prompt for each word, with a subagent's reply after the
    /// first. It's still open, as a hub's is.
    fn hub_with(path: &Path, words: &[&str]) -> Store {
        let store = Store::open(path, &path.with_extension("backups")).unwrap();
        store.add_repo(&"r".into(), "repo", "/repo", 0).unwrap();
        store.insert_workspace(&fixtures::workspace("w"), &WorkspacePlacement::local_checkout("/repo"), None).unwrap();
        let session = fixtures::session("s");
        store.insert_session(&StoredSession { session, agent_session_id: None, agent_opened: false }, None).unwrap();
        let append = |body: &EventBody| drop(store.append(&"s".into(), None, body, None, None, None).unwrap());
        for (i, word) in words.iter().enumerate() {
            let prompt = Prompt::new(format!("Look at `{word}`."));
            append(&EventBody::UserPrompt { client_msg_id: None, prompt, from: PromptFrom::Person });
            if i == 0 {
                let text = ContentBlock::Text { text: "Look at `SubagentOnly`.".into(), images: Vec::new() };
                append(&EventBody::Assistant {
                    message_id: None,
                    subagent: Some("a".into()),
                    model: None,
                    blocks: vec![text],
                    first_index: None,
                });
            }
        }
        store
    }

    /// A recording of something dictated into `chat` with the places `loaded` of it, and heard with `hints`.
    fn recording(chat: &str, loaded: &[(u64, u64)], hints: &[&str]) -> Recording {
        let loaded = loaded.iter().map(|&(first, last)| Loaded { first, last }).collect();
        let picked_from = PickedFrom { typed: "Ask `Balquhidder`".to_string(), chat: loaded, ..PickedFrom::default() };
        let was = Hints::new(hints.iter().copied());
        let hints = was.kept().map(|hint| hint_words::HintWord { text: hint.text, spelled: hint.spelled }).collect();
        let kept = kept("r", 1, &[]);
        Recording::of(Kept { chat: Some(chat.to_string()), picked_from: Some(picked_from), hints, ..kept })
    }

    #[test]
    fn todays_words_are_picked_from_what_the_app_had_loaded_of_the_chat() {
        let data = tempfile::tempdir().unwrap();
        let hub = data.path().join("hub.db");
        let open = hub_with(&hub, &["TunnelPool", "LinkTable", "KeeperRow"]);
        let out = data.path().join("run");
        let chats = Chats::open(&hub, &out).unwrap();

        let loaded = recording("s", &[(1, 1), (3, 3)], &["TunnelPool"]);
        let to_hear = ToHear::of(vec![loaded], TODAYS, Some(&chats)).unwrap();
        let words = to_hear.each[0].1.words();
        for word in ["Balquhidder", "TunnelPool", "KeeperRow"] {
            assert!(words.iter().any(|w| w == word), "{word} in {words:?}");
        }
        for word in ["LinkTable", "SubagentOnly"] {
            assert!(!words.iter().any(|w| w == word), "{word} in {words:?}");
        }
        assert_eq!((to_hear.same, to_hear.gone), (0, 0));

        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let same = recording("s", &[(1, 1), (3, 3)], &words);
        assert_eq!(ToHear::of(vec![same], TODAYS, Some(&chats)).unwrap().same, 1);

        drop(chats);
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "the copy goes");
        assert!(open.get_session(&"s".into()).is_ok());
    }

    #[test]
    fn one_whose_chat_is_gone_or_that_doesnt_say_what_it_was_picked_from_isnt_heard() {
        let data = tempfile::tempdir().unwrap();
        let hub = data.path().join("hub.db");
        let _open = hub_with(&hub, &["TunnelPool", "LinkTable"]);
        let chats = Chats::open(&hub, &data.path().join("run")).unwrap();

        let past_the_end = recording("s", &[(1, 3)], &[]);
        let no_such_chat = recording("other", &[(1, 1)], &[]);
        let unsaid = Recording::of(kept("r", 1, &[]));
        let none_loaded = recording("other", &[], &[]);
        let recordings = vec![past_the_end, no_such_chat, unsaid, none_loaded];
        let to_hear = ToHear::of(recordings, TODAYS, Some(&chats)).unwrap();

        assert_eq!((to_hear.each.len(), to_hear.gone), (1, 3));
        assert_eq!(to_hear.each[0].1.words(), ["Balquhidder"]);
    }

    #[test]
    fn the_newest_of_a_long_chat_are_what_its_words_are_picked_from() {
        let data = tempfile::tempdir().unwrap();
        let hub = data.path().join("hub.db");
        let open = hub_with(&hub, &[]);
        let append = |body: &EventBody| drop(open.append(&"s".into(), None, body, None, None, None).unwrap());
        for place in 1..=2100 {
            if place == 1600 {
                let prompt = Prompt::new("Look at `TunnelPool`.");
                append(&EventBody::UserPrompt { client_msg_id: None, prompt, from: PromptFrom::Person });
            } else {
                append(&EventBody::AgentOther { kind: "k".into(), subagent: None });
            }
        }
        let chats = Chats::open(&hub, &data.path().join("run")).unwrap();
        let long = recording("s", &[(1, 2100)], &[]);
        let to_hear = ToHear::of(vec![long], TODAYS, Some(&chats)).unwrap();
        let words = to_hear.each[0].1.words();
        assert!(words.iter().any(|word| word == "TunnelPool"), "{words:?}");
    }
}
