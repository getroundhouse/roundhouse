//! Signing in to an MCP server from Settings, against a real hub and a stand-in for the server and its authorization
//! server, as `support/tool_servers.rs` has them. Every server and sign-in is made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::TestAppContext;
use hub::{ToolServerAsks, ToolServerPerson, ToolServerRefreshing, ToolServerScript};
use protocol::model::{ToolServerAuth, ToolSignInStatus};
use url::Url;

use super::git_app::label_of;
use super::open_settings;
use super::tool_servers::{
    KEY_WAY, NONE_WAY, SIGN_IN_CHOSEN, choose, fill_in, in_servers, more, named, row, the_hubs, type_in,
};
use crate::support::{SentBack, Serving, Stack, Ui, files_holding, redirect_of, renewing_often, wait_until};

pub(super) const WORKS: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };
pub(super) const NEEDS_SIGN_IN: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn };

/// Waits until nothing listens where `page` sends the person back to.
fn until_stopped(page: &str) {
    let to = Url::parse(&redirect_of(page)).unwrap();
    let at = (to.host_str().unwrap().to_string(), to.port().unwrap());
    wait_until("the listener to stop", || std::net::TcpStream::connect(&at).is_err());
}

/// Waits for the browser to be opened on a page, other than `before`, and for the panel to wait there.
pub(super) fn page_opened(ui: &mut Ui<'_>, before: Option<&str>) -> String {
    ui.wait("the server's page", |ui| ui.cx.opened_url().as_deref() != before && ui.has("mcp-open-page"));
    ui.cx.opened_url().unwrap()
}

/// What the panel says under its title.
pub(super) fn says(ui: &mut Ui<'_>) -> String {
    label_of(ui, "mcp-panel-says")
}

pub(super) fn refused(ui: &mut Ui<'_>) -> String {
    ui.wait("why", |ui| ui.has("mcp-refused"));
    label_of(ui, "mcp-refused")
}

pub(super) fn auth_of(ui: &Ui<'_>, name: &str) -> Option<ToolServerAuth> {
    named(ui, name).map(|server| server.auth)
}

/// One walk through an approval: the panel while it waits, Settings closed and opened again meanwhile, a redirect that
/// isn't this sign-in's, and the one that is.
#[gpui_kit::test]
fn a_server_is_added_once_its_sign_in_is_approved_in_the_browser_whatever_the_window_shows(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, "Linear", &serving.url());
    assert_eq!(says(&mut ui), "Every agent has its tools once it's added.");

    ui.click("mcp-panel-go");

    let page = page_opened(&mut ui, None);
    let machine = if cfg!(target_os = "macos") { "Mac" } else { "computer" };
    let approve = format!(
        "Approve Roundhouse on 127.0.0.1, in the browser on this {machine}. Roundhouse never sees your password."
    );
    assert_eq!(says(&mut ui), approve);
    assert!(ui.off("mcp-add"), "while a panel is open");
    let back = serving.sent_back_to(&page);
    let another = SentBack { to: back.to.clone(), query: "code=a-made-up-code&state=another".to_string() };
    assert_eq!(another.follow().status, 400);
    ui.click("settings-back");
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("its panel again", |ui| ui.has("mcp-open-page"));
    assert_eq!(the_hubs(&stack), Vec::<String>::new(), "nothing is added until it's approved");
    ui.click("mcp-open-page");
    assert_eq!(ui.cx.opened_url(), Some(page.clone()));

    let answered = back.follow();

    assert_eq!(answered.status, 200);
    assert!(answered.body.contains("Go back to Roundhouse"), "{}", answered.body);
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
    let linear = named(&ui, "Linear").unwrap();
    assert!(!ui.has(format!("{}-says", row(&linear.id))), "one that works says nothing more");
    assert_eq!(serving.stand_in.asked_for("POST /token").len(), 1);
    until_stopped(&page);
    let code = back.query.split('&').find_map(|said| said.strip_prefix("code=")).unwrap();
    assert_eq!(files_holding(stack.dir.path(), code, ""), Vec::<PathBuf>::new(), "what came back isn't kept");
}

#[gpui_kit::test]
fn a_sign_in_thats_refused_says_so_and_can_be_changed_and_tried_again(cx: &mut TestAppContext) {
    let script = ToolServerScript { person: ToolServerPerson::Refuses, ..ToolServerScript::default() };
    let (stack, serving) = (Stack::new(), Serving::with(script));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, "Linear", "mcp.example.com/mcp");
    ui.press("enter");
    let not_a_url = "Couldn't sign in: an MCP server's URL starts with http:// or https://";
    assert_eq!(refused(&mut ui), not_a_url, "before the hub asks anyone");
    type_in(&mut ui, "mcp-url", &serving.url());
    ui.click("mcp-panel-go");
    let page = page_opened(&mut ui, None);

    let answered = serving.sent_back_to(&page).follow();

    assert!(answered.body.contains("Go back to Roundhouse, which says why."), "{}", answered.body);
    assert_eq!(refused(&mut ui), "Couldn't sign in: the sign-in was refused");
    assert_eq!(the_hubs(&stack), Vec::<String>::new());
    until_stopped(&page);
    serving.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click("mcp-panel-go");
    let again = page_opened(&mut ui, Some(&page));
    serving.sent_back_to(&again).follow();
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
}

/// One that anyone can use, then one that can't be signed in to, each added as the panel says it may be.
#[gpui_kit::test]
fn a_new_server_with_no_sign_in_says_so_in_the_panel_it_was_typed_in_and_is_added_another_way(cx: &mut TestAppContext) {
    let open = ToolServerScript { asks: ToolServerAsks::Nothing, ..ToolServerScript::default() };
    let registered = ToolServerScript { client_documents: false, ..ToolServerScript::default() };
    let (stack, open, registered) = (Stack::new(), Serving::with(open), Serving::with(registered));
    let only_registered = format!(
        "Roundhouse can't sign in to this server. {}, where it's signed in to, takes only apps registered with it. It \
         may take a key.",
        registered.stand_in.issuer()
    );
    let no_sign_in = "This server has no sign-in. Anyone who can reach it can use it. To add it, choose None, or Key if \
                      it gave you one.";
    let mut ui = in_servers(cx, &stack);
    let refused_as_it_was_typed = |ui: &mut Ui<'_>, name: &str, serving: &Serving, why: &str| {
        fill_in(ui, name, &serving.url());
        ui.click("mcp-panel-go");
        assert_eq!(refused(ui), why);
        assert_eq!(label_of(ui, "mcp-way"), SIGN_IN_CHOSEN, "it doesn't move by itself");
        assert!(ui.on("mcp-panel-go"), "what was typed is there");
        assert_eq!(ui.cx.opened_url(), None);
        assert!(named(ui, name).is_none(), "nothing is added");
    };

    refused_as_it_was_typed(&mut ui, "Docs", &open, no_sign_in);

    choose(&mut ui, NONE_WAY);
    assert!(!ui.has("mcp-refused"));
    ui.click("mcp-panel-go");
    ui.wait("the server anyone can use", |ui| named(ui, "Docs").is_some() && !ui.has("mcp-panel"));
    let docs = named(&ui, "Docs").unwrap();
    assert_eq!((docs.url.as_str(), docs.auth), (open.url().as_str(), ToolServerAuth::Neither));

    refused_as_it_was_typed(&mut ui, "Runbooks", &registered, &only_registered);

    choose(&mut ui, KEY_WAY);
    ui.input("a-made-up-key");
    ui.press("enter");
    ui.wait("the server with a key", |ui| named(ui, "Runbooks").is_some() && !ui.has("mcp-panel"));
    let runbooks = named(&ui, "Runbooks").unwrap();
    assert_eq!(runbooks.url.as_str(), registered.url());
    assert!(matches!(runbooks.auth, ToolServerAuth::Key { .. }));
}

/// Its server is the hub's already, so there's no way to choose.
#[gpui_kit::test]
fn a_server_signed_in_to_again_with_no_way_to_says_only_why_and_closes(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    let linear = serving.signed_in_elsewhere(&stack);
    ui.wait("the server", |ui| ui.has(more(&linear)));
    let only_why = |ui: &mut Ui<'_>, why: &str| {
        ui.click(more(&linear));
        ui.click_menu_item("Sign in again");
        ui.wait("why", |ui| ui.has("mcp-close"));
        assert_eq!(says(ui), why);
        assert!(!ui.has("mcp-refused") && !ui.has("mcp-way") && !ui.has("mcp-try-again"));
        ui.click("mcp-close");
        assert!(!ui.has("mcp-panel"));
    };
    let only_registered = format!(
        "Roundhouse can't sign in to this server. {}, where it's signed in to, takes only apps registered with it.",
        serving.stand_in.issuer()
    );

    serving.stand_in.change(|script| script.client_documents = false);
    only_why(&mut ui, &only_registered);
    serving.stand_in.change(|script| script.asks = ToolServerAsks::Nothing);
    only_why(&mut ui, "This server has no sign-in. Anyone who can reach it can use it.");
}

#[gpui_kit::test]
fn cancelling_a_sign_in_that_waits_puts_its_panel_away_and_nothing_listens(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, "Linear", &serving.url());
    ui.click("mcp-panel-go");
    let page = page_opened(&mut ui, None);

    ui.click("mcp-cancel");

    assert!(!ui.has("mcp-panel"));
    until_stopped(&page);
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack), Vec::<String>::new());
    assert!(ui.on("mcp-add"));
}

/// Its tokens end sooner than the hub renews one, so each check does, until the authorization server refuses one.
#[gpui_kit::test]
fn a_server_that_needs_it_is_signed_in_to_again_from_its_menu_and_a_failure_is_tried_again(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::in_environment(&renewing_often()), Serving::with_short_tokens());
    let mut ui = in_servers(cx, &stack);
    let linear = serving.signed_in_elsewhere(&stack);
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS));
    serving.refuses_to_renew();
    ui.wait("it to need signing in to", |ui| auth_of(ui, "Linear") == Some(NEEDS_SIGN_IN));
    assert!(ui.shows_label(&row(&linear), "Needs you to sign in"));
    serving.stand_in.change(|script| {
        (script.refreshing, script.person) = (ToolServerRefreshing::Works, ToolServerPerson::Refuses);
    });

    ui.click(more(&linear));
    ui.click_menu_item("Sign in again");

    let page = page_opened(&mut ui, None);
    serving.sent_back_to(&page).follow();
    assert_eq!(refused(&mut ui), "Couldn't sign in: the sign-in was refused");
    assert!(ui.has("mcp-close"));
    serving.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click("mcp-try-again");
    let again = page_opened(&mut ui, Some(&page));
    serving.sent_back_to(&again).follow();
    ui.wait("its sign-in to work", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
    assert_eq!(named(&ui, "Linear").unwrap().id, linear, "the same server");
    assert!(!ui.has(format!("{}-says", row(&linear))));
}
