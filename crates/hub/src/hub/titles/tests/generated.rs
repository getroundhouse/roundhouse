//! A title the agent gives for a chat's first prompt, which replaces its first line wherever that's still the title,
//! and gives one to a chat and a workspace whose prompt had no words.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::attachment::MediaType;
use protocol::client::HubNotice;
use protocol::model::TitleSource;
use protocol::prompt::Prompt;

use super::titling::{Named, answering, held, images, png, within};

/// A prompt such as "how do we fix this?" can be about a screenshot, so the agent is given its images, and only them,
/// with the whole of what it says.
#[tokio::test]
async fn a_generated_title_replaces_the_first_line_of_the_workspace_and_its_chat_from_the_prompt_and_its_images() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    named.answers(answering("Fix the login redirect"));
    let (made, chat) = (named.made.clone(), named.chat.id.clone());
    let (shot, log) = (named.add("shot.png", MediaType::Png).await, named.add("build.log", MediaType::Text).await);

    named.prompt(&chat, Prompt::carrying("fix the login\nit loops", vec![shot.id.clone(), log.id])).await;

    named.generated(&chat).await;
    let titled = named.workspace();
    assert_eq!(
        (titled.title.as_deref(), titled.title_source),
        (Some("Fix the login redirect"), TitleSource::Generated)
    );
    assert_eq!(named.session(&chat).title.as_deref(), Some("Fix the login redirect"));
    assert_eq!((titled.name, titled.branch), (made.name, made.branch));
    let [asked] = &named.calls()[..] else { panic!("one call for both") };
    assert_eq!(asked.prompt, "fix the login\nit loops", "the whole prompt, not only its first line");
    assert_eq!(images(asked), [(shot, png())], "the image, and not the log");
}

/// The person renames the workspace, or the chat, while the agent is asked for a title, which the hub takes as it comes,
/// and the title then goes only to the other.
#[tokio::test(flavor = "multi_thread")]
async fn a_title_typed_while_one_is_generated_stays() {
    for renaming_the_workspace in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let named = Named::new(dir.path()).await;
        let (made, chat) = (named.made.id.clone(), named.chat.id.clone());
        let (let_go, answer) = held("Fix the login redirect");
        named.answers(answer);
        let mut feed = named.hub.subscribe_feed();
        named.prompt(&chat, Prompt::new("fix the login")).await;
        named.asked_for(1).await;

        let (renamed, typed) = if renaming_the_workspace {
            let renamed = within(named.hub.workspace_rename(made, "Login bug")).await.unwrap();
            ((renamed.title, renamed.title_source), "Login bug")
        } else {
            let renamed = within(named.hub.session_rename(chat.clone(), "First try")).await.unwrap();
            ((renamed.title, renamed.title_source), "First try")
        };
        let_go.now();

        assert_eq!((renamed.0.as_deref(), renamed.1), (Some(typed), TitleSource::ByHand), "{renaming_the_workspace}");
        // Told of the other's generated title once both are saved.
        let told = async {
            loop {
                let notice = std::sync::Arc::unwrap_or_clone(feed.recv().await.unwrap());
                let generated = if renaming_the_workspace {
                    matches!(&notice, HubNotice::Session(s) if s.title_source == TitleSource::Generated)
                } else {
                    matches!(&notice, HubNotice::Workspace(w) if w.title_source == TitleSource::Generated)
                };
                if generated {
                    break;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), told).await.expect("the other's generated title");
        let (workspace, chat) = (named.workspace(), named.session(&chat));
        let [kept, generated] = if renaming_the_workspace {
            [(workspace.title, workspace.title_source), (chat.title, chat.title_source)]
        } else {
            [(chat.title, chat.title_source), (workspace.title, workspace.title_source)]
        };
        assert_eq!((kept.0.as_deref(), kept.1), (Some(typed), TitleSource::ByHand), "{renaming_the_workspace}");
        assert_eq!((generated.0.as_deref(), generated.1), (Some("Fix the login redirect"), TitleSource::Generated));
    }
}

/// With no words there's no first line, so nothing is shown in its place while the title is on its way.
#[tokio::test]
async fn a_prompt_of_only_an_image_is_titled_once_its_generated_title_comes() {
    let dir = tempfile::tempdir().unwrap();
    let named = Named::new(dir.path()).await;
    let (store, made, chat) = (named.hub.store.clone(), named.made.id.clone(), named.chat.id.clone());
    let meanwhile = Arc::new(Mutex::new(None));
    let (seen, session) = (meanwhile.clone(), chat.clone());
    named.answers(move |_| {
        let workspace = store.get_workspace(&made).unwrap().title;
        *seen.lock() = Some((workspace, store.get_session(&session).unwrap().session.title));
        Ok("Fix the queue error".to_string())
    });
    let shot = named.add("shot.png", MediaType::Png).await;

    named.prompt(&chat, Prompt::carrying("", vec![shot.id.clone()])).await;

    named.generated(&chat).await;
    assert_eq!(*meanwhile.lock(), Some((None, None)));
    let titled = named.workspace();
    assert_eq!((titled.title.as_deref(), titled.title_source), (Some("Fix the queue error"), TitleSource::Generated));
    assert_eq!(named.session(&chat).title.as_deref(), Some("Fix the queue error"));
    let [asked] = &named.calls()[..] else { panic!("one call for both") };
    assert_eq!(images(asked), [(shot, png())]);
}
