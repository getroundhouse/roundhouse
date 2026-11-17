//! A title from a prompt's first line, and one typed by hand, which the first prompt leaves alone.

use protocol::client::ErrorCode;
use protocol::model::{TitleSource, WorkspaceId};
use protocol::prompt::Prompt;

use super::titling::{Named, no_title};

/// Here the agent gives no title, so each keeps its first line.
#[tokio::test]
async fn the_first_prompt_titles_the_workspace_and_each_chat() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    named.answers(|_| Err(no_title()));
    let (made, first) = (named.made.clone(), named.chat.id.clone());
    assert_eq!((made.title.as_deref(), made.label()), (None, made.name.as_str()), "the name until a prompt");

    named.prompt(&first, Prompt::new("fix the login\nand then the rest")).await;
    let second = named.another_chat().await;
    named.prompt(&second.id, Prompt::new("write the docs")).await;

    named.asked_for(2).await;
    let titled = named.workspace();
    assert_eq!((titled.title.as_deref(), titled.title_source), (Some("fix the login"), TitleSource::FirstLine));
    assert_eq!((titled.name, titled.branch), (made.name, made.branch), "a title leaves the name alone");
    assert_eq!(named.session(&first).title.as_deref(), Some("fix the login"));
    assert_eq!(named.session(&second.id).title.as_deref(), Some("write the docs"));
}

#[tokio::test]
async fn a_title_typed_by_hand_stays_through_the_first_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    named.answers(|_| Err(no_title()));
    let (made, chat) = (named.made.clone(), named.chat.id.clone());

    let renamed = named.hub.workspace_rename(made.id.clone(), "  Login\n bug ").await.unwrap();
    assert_eq!((renamed.title.as_deref(), renamed.title_source), (Some("Login bug"), TitleSource::ByHand));
    let renamed = named.hub.session_rename(chat.clone(), "First try").await.unwrap();
    assert_eq!((renamed.title.as_deref(), renamed.title_source), (Some("First try"), TitleSource::ByHand));
    named.prompt(&chat, Prompt::new("fix the login")).await;

    assert_eq!(named.workspace().title.as_deref(), Some("Login bug"));
    assert_eq!(named.session(&chat).title.as_deref(), Some("First try"));
    assert!(named.workspace().rev > made.rev);
}

#[tokio::test]
async fn renaming_to_nothing_or_something_missing_is_refused_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    named.answers(|_| Err(no_title()));
    let (made, chat) = (named.made.id.clone(), named.chat.id.clone());
    named.hub.workspace_rename(made.clone(), "Kept").await.unwrap();
    named.hub.session_rename(chat.clone(), "Kept too").await.unwrap();

    let empty = named.hub.workspace_rename(made, " \n ").await.map_err(|e| e.code);
    let missing_workspace = named.hub.workspace_rename(WorkspaceId::from("missing"), "x").await.map_err(|e| e.code);
    let missing_chat = named.hub.session_rename("missing".into(), "x").await.map_err(|e| e.code);

    assert_eq!(empty.err(), Some(ErrorCode::InvalidRequest));
    assert_eq!(missing_workspace.err(), Some(ErrorCode::NotFound));
    assert_eq!(missing_chat.err(), Some(ErrorCode::NotFound));
    assert_eq!(named.workspace().title.as_deref(), Some("Kept"));
    assert_eq!(named.session(&chat).title.as_deref(), Some("Kept too"));
}
