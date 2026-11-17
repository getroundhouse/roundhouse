//! A workspace titled again when someone asks, from everything its chats were sent.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::attachment::MediaType;
use protocol::client::ErrorCode;
use protocol::model::{Session, TitleSource, WorkspaceId};
use protocol::prompt::Prompt;

use super::titling::{Named, held, images, no_title, png, within};

/// A workspace with a generated title from its first chat's prompt, which carried an image, and a second chat with a
/// prompt of its own, both titled `Fix the login redirect`, whose worker answers a call for a title with what `title`
/// holds then.
async fn worked_in(named: &Named, title: &Arc<Mutex<String>>) -> Session {
    let answer = title.clone();
    named.answers(move |_| Ok(answer.lock().clone()));
    *title.lock() = "Fix the login redirect".to_string();
    let shot = named.add("shot.png", MediaType::Png).await;
    named.prompt(&named.chat.id, Prompt::carrying("fix the login\nit loops", vec![shot.id])).await;
    named.generated(&named.chat.id).await;
    let second = named.another_chat().await;
    named.prompt(&second.id, Prompt::new("now add a test")).await;
    named.generated(&second.id).await;
    second
}

#[tokio::test]
async fn asking_for_a_new_title_names_the_workspace_from_all_its_prompts_and_images_even_over_one_typed_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    let title = Arc::new(Mutex::new(String::new()));
    worked_in(&named, &title).await;
    let made = named.workspace();
    named.hub.workspace_rename(made.id.clone(), "Login bug").await.unwrap();
    *title.lock() = "Fix and test the login".to_string();

    let retitled = named.hub.workspace_regenerate_title(made.id.clone()).await.unwrap();

    let now = (retitled.title.as_deref(), retitled.title_source);
    assert_eq!(now, (Some("Fix and test the login"), TitleSource::Generated));
    assert_eq!(named.workspace(), retitled, "what it answers is what's kept");
    assert_eq!((retitled.name, retitled.branch), (made.name, made.branch), "a title leaves the name alone");
    let asked = named.calls().pop().unwrap();
    let prompts = "\n\n1. fix the login it loops\n2. now add a test";
    assert!(asked.prompt.ends_with(prompts), "every chat's prompts, oldest first: {}", asked.prompt);
    assert_eq!(images(&asked).into_iter().map(|(_, data)| data).collect::<Vec<_>>(), [png()], "and their images");
    let chat = named.session(&named.chat.id);
    assert_eq!(chat.title.as_deref(), Some("Fix the login redirect"), "not its chats");
}

/// The person renames it while the agent is asked, which the hub takes as it comes, so what they typed stays, and is
/// what the hub answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_title_typed_while_a_new_one_is_asked_for_stays() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    let title = Arc::new(Mutex::new(String::new()));
    worked_in(&named, &title).await;
    let (let_go, answer) = held("Fix and test the login");
    named.answers(answer);
    let (hub, made) = (named.hub.clone(), named.made.id.clone());
    let asking = tokio::spawn(async move { hub.workspace_regenerate_title(made).await });
    named.asked_for(3).await;

    let renamed = within(named.hub.workspace_rename(named.made.id.clone(), "Login bug")).await.unwrap();
    let_go.now();

    assert_eq!((renamed.title.as_deref(), renamed.title_source), (Some("Login bug"), TitleSource::ByHand));
    let answered = asking.await.unwrap().unwrap();
    assert_eq!((answered.title.as_deref(), answered.title_source), (Some("Login bug"), TitleSource::ByHand));
    assert_eq!(named.workspace().title.as_deref(), Some("Login bug"));
}

#[tokio::test]
async fn asking_for_a_new_title_is_refused_with_nothing_to_go_on_and_says_when_none_comes() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    named.answers(|_| Err(no_title()));
    let made = named.made.id.clone();

    let unprompted = named.hub.workspace_regenerate_title(made.clone()).await.map_err(|e| e.code);
    let missing = named.hub.workspace_regenerate_title(WorkspaceId::from("missing")).await.map_err(|e| e.code);
    named.prompt(&named.chat.id, Prompt::new("fix the login")).await;
    named.asked_for(1).await;
    let failed = named.hub.workspace_regenerate_title(made.clone()).await.map_err(|e| e.code);
    named.hub.workspace_archive(made.clone(), true).await.unwrap();
    let before = named.calls().len();
    let archived = named.hub.workspace_regenerate_title(made.clone()).await.map_err(|e| e.code);

    // Refused as it's archived, rather than as its worker couldn't be started, which this hub's provider refuses once
    // archiving has stopped the stand-in.
    assert_eq!(archived.err(), Some(ErrorCode::Conflict));
    assert!(!named.hub.workers.lock().get(&made).is_some_and(|link| link.is_open()), "and starts no worker for it");
    assert_eq!(named.calls().len(), before, "nor asks one");
    assert_eq!(unprompted.err(), Some(ErrorCode::Conflict));
    assert_eq!(missing.err(), Some(ErrorCode::NotFound));
    assert_eq!(failed.err(), Some(ErrorCode::Worker));
    let kept = named.workspace();
    assert_eq!((kept.title.as_deref(), kept.title_source), (Some("fix the login"), TitleSource::FirstLine));
}
