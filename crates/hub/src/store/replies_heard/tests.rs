use protocol::model::{Ending, ReplyPlace};

use crate::store::tests::store_with_session;

/// The first thing said of a reply is kept, so a device sending it again, or another of the same reply, changes
/// nothing, and each reply is listed with the newest revision of them all.
#[test]
fn the_first_thing_said_of_a_reply_is_kept_and_listed_by_its_chat() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let cut = Ending::TalkedOver { through: ReplyPlace { block: 1, through: 40 } };

    let first = store.keep_reply_heard(&"s".into(), &"m1".into(), cut).unwrap().expect("kept");
    let again = store.keep_reply_heard(&"s".into(), &"m1".into(), Ending::ToTheEnd).unwrap();
    let second = store.keep_reply_heard(&"s".into(), &"m2".into(), Ending::NotReadOut).unwrap().expect("kept");
    let (listed, rev) = store.list_replies_heard(&"s".into()).unwrap();

    assert_eq!(again, None);
    assert_eq!(first.ending, cut);
    assert!(second.rev.supersedes(first.rev));
    assert_eq!(listed, [first, second.clone()]);
    assert_eq!(rev, second.rev);
}
