//! Making an app of your own from GitHub's code against a stand-in hub, which refuses the first time as if GitHub
//! couldn't be reached, so a test can see what Try again gives it.

use std::sync::mpsc;

use client_core::stand_in_hub::Does;
use gpui_kit::TestAppContext;
use protocol::client::github_apps::GithubManifestCode;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::fixtures::github_app;
use protocol::model::GithubAppId;

use super::super::app_panel::{AppStep, Chosen, OnGithub};
use super::super::kept::Kept;
use super::stand_in::{WithStandIn, until};
use crate::manifest_flow::Under;

/// Why the hub couldn't make the app, and whether the panel offers the same code again, once it says.
fn not_made(kept: &Kept) -> Option<(String, bool)> {
    let Some(AppStep::OnGithub { stands: OnGithub::Making { failed: Some(failed), .. }, .. }) =
        kept.app_panel().map(|app| &app.step)
    else {
        return None;
    };
    Some((failed.why.clone(), failed.again))
}

#[gpui_kit::test]
fn trying_again_gives_the_hub_the_same_code_and_goes_on_to_installing_once_its_made(cx: &mut TestAppContext) {
    let (asked, codes) = mpsc::channel();
    let mut tries = 0;
    let page = WithStandIn::new(cx, move |request| {
        let ClientRequest::GithubAppMake(make) = request else { return Does::Nothing };
        asked.send(make.code.as_str().to_string()).unwrap();
        tries += 1;
        let answer = if tries == 1 {
            ReplyOutcome::Error(ApiError::new(ErrorCode::Unavailable, "GitHub didn't answer in time"))
        } else {
            ReplyOutcome::Response(ClientResponse::GithubApp(github_app(9)))
        };
        Does::Answers(Box::new(answer))
    });
    let chosen = Chosen { under: Under::Account, name: "ada-roundhouse".to_string() };
    let making = OnGithub::Making { code: GithubManifestCode::new("a-made-up-code"), failed: None };
    page.act(cx, |kept, window, cx| {
        kept.open_app_at(AppStep::OnGithub { chosen, stands: making }, cx);
        kept.make(window, cx);
    });
    until(cx, "the refusal", |cx| page.kept.read_with(cx, |kept, _| not_made(kept).is_some()));
    let (why, again) = page.kept.read_with(cx, |kept, _| not_made(kept).unwrap());
    assert!(why.contains("GitHub didn't answer in time") && again, "{why}");

    page.act(cx, Kept::make);

    let installing = |kept: &Kept| matches!(kept.app_panel().map(|app| &app.step), Some(AppStep::Installing { app, .. }) if *app == GithubAppId::new(9));
    until(cx, "the install step", |cx| page.kept.read_with(cx, |kept, _| installing(kept)));
    assert_eq!(codes.try_iter().collect::<Vec<_>>(), ["a-made-up-code", "a-made-up-code"], "the same code, twice");
}
