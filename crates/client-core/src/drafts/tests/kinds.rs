//! A file of a kind the app knows and its hub doesn't, which the hub refuses as it would a request it doesn't know.

use super::*;

#[test]
fn a_file_of_a_kind_the_hub_doesnt_know_goes_to_a_chat_as_one_that_goes_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::KnowsNoKindOfFile);
    let drafts = Drafts::start(&client);

    drafts.attach(&chat("a"), file("f1"));
    drafts.set(&chat("a"), NewDraft { text: "look".to_string(), files: vec!["f1".into()] });

    let said = pairs(&[("a", "refused +f1"), ("a", "+f1"), ("a", "look f1")]);
    assert_eq!(kept(&drafts, &heard), said, "the draft goes on");
    assert!(!drafts.unkept(&chat("a")));
}
