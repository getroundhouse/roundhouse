//! Editing an MCP server in Settings, against a real hub, and stand-ins for servers: what's kept,
//! what's asked for again, and what the panel says running agents have of it. Every server, key and sign-in is plainly
//! made up.

#![cfg(test)]

use gpui_kit::TestAppContext;
use hub::{ToolServerAsks, ToolServerPerson, ToolServerScript};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{ToolServerAuth, ToolServerAuthGiven, ToolServerId};

use super::git_app::label_of;
use super::tool_servers::{
    KEY, KEY_WAY, NONE_WAY, SIGN_IN_WAY, added_elsewhere, choose, fill_in, in_servers, more, named, open_to_anyone,
    row, the_hubs, type_in, with_key,
};
use super::tool_sign_in::{NEEDS_SIGN_IN, WORKS, auth_of, page_opened, refused, says};
use crate::support::{Serving, Stack, Ui, files_holding, renewing_often};

const ANOTHER_SIGN_IN: &str = "That's another server, so you sign in to it. Nothing changes until you have.";
const LOST: &str = "Running agents lose this server until their next start.";

pub(super) fn edit(ui: &mut Ui<'_>, id: &ToolServerId) {
    ui.click(more(id));
    ui.click_menu_item("Edit");
    ui.wait("its panel", |ui| ui.has("mcp-panel-go"));
}

/// Saves what's typed, and waits for the panel to go.
fn save(ui: &mut Ui<'_>) {
    assert_eq!(label_of(ui, "mcp-panel-go"), "Save");
    ui.click("mcp-panel-go");
    ui.wait("it to be saved", |ui| !ui.has("mcp-panel"));
}

fn url_of(ui: &Ui<'_>, name: &str) -> String {
    named(ui, name).unwrap().url.as_str().to_owned()
}

/// How many of the hub's files have `secret`.
fn kept(stack: &Stack, secret: &str) -> usize {
    files_holding(stack.dir.path(), secret, "").len()
}

#[gpui_kit::test]
fn a_server_is_edited_under_its_row_and_its_new_name_is_for_agents_next_start(cx: &mut TestAppContext) {
    let (stack, serving, elsewhere) = (Stack::new(), open_to_anyone(), open_to_anyone());
    let url = serving.url();
    let mut ui = in_servers(cx, &stack);
    let docs = added_elsewhere(&stack, "docs", "Docs", &url, ToolServerAuthGiven::Neither);
    let server = added_elsewhere(&stack, "runbooks", "Runbooks", &url, ToolServerAuthGiven::Neither);
    ui.wait("its row", |ui| ui.has(more(&server)));
    edit(&mut ui, &docs);
    assert_eq!(label_of(&mut ui, "mcp-url-says"), "You've already added this one, as Runbooks.", "and not as itself");
    ui.click(more(&server));
    ui.press("down");
    ui.press("enter");
    ui.wait("what's on in its menu", |ui| ui.asked().is_some());
    assert_eq!(ui.asked().unwrap().0, "Remove Runbooks?", "Edit is off while a panel is open");
    ui.click("remove-mcp-server-keep");
    ui.click("mcp-cancel");

    edit(&mut ui, &server);

    assert_eq!(says(&mut ui), "Change its name, its URL or how it knows you.");
    assert!(ui.bounds("mcp-panel").origin.y > ui.bounds(more(&server)).origin.y, "under its row");
    assert!(ui.off("mcp-add"), "while a panel is open");
    assert_eq!(label_of(&mut ui, "mcp-way"), "Authentication: None");
    assert_eq!(label_of(&mut ui, "mcp-url-says"), "You've already added this one, as Docs.");
    ui.press("secondary-a");
    ui.input("Docs");
    assert_eq!(says(&mut ui), "Running agents have the new name from their next start.");
    ui.press("enter");
    let taken = "Couldn't save it: another MCP server is called Docs";
    assert_eq!(refused(&mut ui), taken);
    type_in(&mut ui, "mcp-name", "Northwind runbooks");
    type_in(&mut ui, "mcp-url", &format!("{url}?tools=a"));
    assert_eq!(says(&mut ui), "Running agents have the new name from their next start, and the rest at once.");
    save(&mut ui);

    ui.wait("its new name", |ui| named(ui, "Northwind runbooks").is_some());
    assert_eq!(
        (url_of(&ui, "Northwind runbooks"), named(&ui, "Docs").unwrap().url.as_str()),
        (format!("{url}?tools=a"), url.as_str())
    );
    edit(&mut ui, &server);
    let renamed = ToolServerEdit { id: server.clone(), name: Some("Elsewhere".into()), url: None, auth: None };
    stack.client().tool_server_edit(renamed).unwrap();
    ui.wait("its name from another app", |ui| named(ui, "Elsewhere").is_some());
    type_in(&mut ui, "mcp-url", &elsewhere.url());
    save(&mut ui);
    assert_eq!(url_of(&ui, "Elsewhere"), elsewhere.url(), "its name, which this app didn't change, stays");
    stack.client().tool_server_remove(&server).unwrap();
    ui.wait("its row gone", |ui| !ui.has(row(&server)));
}

/// With its header changed, and then its query.
#[gpui_kit::test]
fn a_key_left_empty_is_kept_until_its_for_another_server(cx: &mut TestAppContext) {
    let (stack, serving, moved) = (Stack::new(), open_to_anyone(), open_to_anyone());
    let (url, elsewhere) = (serving.url(), moved.url());
    let mut ui = in_servers(cx, &stack);
    let server = added_elsewhere(&stack, "runbooks", "Runbooks", &url, with_key());
    ui.wait("its row", |ui| ui.has(more(&server)));
    let header = |ui: &Ui<'_>| match named(ui, "Runbooks").unwrap().auth {
        ToolServerAuth::Key { header } => header.as_str().to_owned(),
        other => panic!("{other:?}"),
    };

    edit(&mut ui, &server);
    assert_eq!(label_of(&mut ui, "mcp-key-says"), "Leave it empty to keep the one you gave.");
    type_in(&mut ui, "mcp-header", "X-Key");
    assert_eq!(says(&mut ui), "Running agents have the change at once.");
    save(&mut ui);
    ui.wait("its header", |ui| header(ui) == "X-Key");
    assert_eq!(kept(&stack, KEY), 1);

    edit(&mut ui, &server);
    type_in(&mut ui, "mcp-url", &elsewhere);
    assert_eq!(label_of(&mut ui, "mcp-key-says"), "That's another server, so give it its key.");
    assert_eq!(says(&mut ui), LOST);
    assert!(ui.off("mcp-panel-go"), "until there's its key");
    type_in(&mut ui, "mcp-url", &format!("{url}?tools=a"));
    assert!(ui.on("mcp-panel-go"), "it's the same server again");
    type_in(&mut ui, "mcp-url", &elsewhere);
    type_in(&mut ui, "mcp-key", "another-made-up-one");
    save(&mut ui);

    ui.wait("its URL", |ui| url_of(ui, "Runbooks") == elsewhere);
    assert_eq!((header(&ui).as_str(), kept(&stack, KEY), kept(&stack, "another-made-up-one")), ("X-Key", 0, 1));
}

/// Its query, then how it's signed in to, which is forgotten once the key is saved.
#[gpui_kit::test]
fn a_server_stays_signed_in_to_as_its_query_changes_and_until_another_way_is_saved(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    let linear = serving.signed_in_elsewhere(&stack);
    ui.wait("its row", |ui| ui.has(more(&linear)));
    let queried = format!("{}?toolsets=core", serving.url());

    edit(&mut ui, &linear);
    assert_eq!(label_of(&mut ui, "mcp-way-says"), "You stay signed in.");
    type_in(&mut ui, "mcp-url", &queried);
    assert_eq!(label_of(&mut ui, "mcp-way-says"), "You stay signed in.");
    assert_eq!(says(&mut ui), "Running agents have the change at once.");
    save(&mut ui);
    ui.wait("its URL", |ui| url_of(ui, "Linear") == queried);
    assert_eq!(auth_of(&ui, "Linear"), Some(WORKS));
    assert_eq!(serving.stand_in.asked_for("POST /token").len(), 1, "it's the sign-in there was");

    edit(&mut ui, &linear);
    choose(&mut ui, KEY_WAY);
    assert_eq!(label_of(&mut ui, "mcp-key-says"), "What the server gave you.");
    assert!(ui.off("mcp-panel-go"), "until there's a key");
    assert_eq!(auth_of(&ui, "Linear"), Some(WORKS), "nothing is forgotten until it's saved");
    ui.input(KEY);
    serving.stand_in.change(|script| script.asks = ToolServerAsks::Nothing);
    save(&mut ui);

    ui.wait("its key", |ui| matches!(auth_of(ui, "Linear"), Some(ToolServerAuth::Key { .. })));
    assert_eq!(kept(&stack, KEY), 1);
    assert_eq!(std::fs::read_dir(stack.hub.paths.data_dir.join("tool-servers")).unwrap().count(), 1, "and its alone");
    edit(&mut ui, &linear);
    choose(&mut ui, NONE_WAY);
    save(&mut ui);
    ui.wait("neither", |ui| auth_of(ui, "Linear") == Some(ToolServerAuth::Neither));
    assert_eq!(kept(&stack, KEY), 0);
}

#[gpui_kit::test]
fn a_server_that_needs_signing_in_to_isnt_said_to_stay_signed_in_and_still_does_once_its_saved(
    cx: &mut TestAppContext,
) {
    let (stack, serving) = (Stack::in_environment(&renewing_often()), Serving::with_short_tokens());
    let mut ui = in_servers(cx, &stack);
    let linear = serving.signed_in_elsewhere(&stack);
    serving.refuses_to_renew();
    ui.wait("it to need signing in to", |ui| auth_of(ui, "Linear") == Some(NEEDS_SIGN_IN));

    edit(&mut ui, &linear);
    let way = "Your browser opens on its own page, and the hub renews the sign-in by itself.";
    assert_eq!(label_of(&mut ui, "mcp-way-says"), way);
    type_in(&mut ui, "mcp-name", "Linear (Ada)");
    save(&mut ui);

    ui.wait("its name", |ui| auth_of(ui, "Linear (Ada)") == Some(NEEDS_SIGN_IN));
}

/// Refused first, which is the panel again, as it was typed.
#[gpui_kit::test]
fn a_server_thats_signed_in_to_moves_once_its_signed_in_to_where_it_moves(cx: &mut TestAppContext) {
    let script = ToolServerScript { person: ToolServerPerson::Refuses, ..ToolServerScript::default() };
    let (stack, serving, moved) = (Stack::new(), Serving::with(ToolServerScript::default()), Serving::with(script));
    let mut ui = in_servers(cx, &stack);
    let linear = serving.signed_in_elsewhere(&stack);
    ui.wait("its row", |ui| ui.has(more(&linear)));
    edit(&mut ui, &linear);
    type_in(&mut ui, "mcp-url", &moved.url());
    type_in(&mut ui, "mcp-name", "Linear (Europe)");
    let typed = |ui: &mut Ui<'_>| {
        assert_eq!(label_of(ui, "mcp-way-says"), ANOTHER_SIGN_IN);
        assert_eq!(says(ui), "Once you've signed in, running agents lose this server until their next start.");
        assert_eq!(label_of(ui, "mcp-panel-go"), "Sign in");
        assert!(ui.bounds("mcp-panel").origin.y > ui.bounds(more(&linear)).origin.y, "under its row");
    };
    typed(&mut ui);

    ui.click("mcp-panel-go");
    let page = page_opened(&mut ui, None);
    assert!(ui.bounds("mcp-panel").origin.y > ui.bounds(more(&linear)).origin.y, "its sign-in is there too");
    moved.sent_back_to(&page).follow();

    assert_eq!(refused(&mut ui), "Couldn't sign in: the sign-in was refused");
    typed(&mut ui);
    assert_eq!((url_of(&ui, "Linear"), auth_of(&ui, "Linear")), (serving.url(), Some(WORKS)), "nothing changes");
    moved.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click("mcp-panel-go");
    let again = page_opened(&mut ui, Some(&page));
    assert_eq!(the_hubs(&stack), ["Linear"], "until it's approved");
    moved.sent_back_to(&again).follow();

    ui.wait("it to have moved", |ui| named(ui, "Linear (Europe)").is_some() && !ui.has("mcp-panel"));
    assert_eq!((url_of(&ui, "Linear (Europe)"), auth_of(&ui, "Linear (Europe)")), (moved.url(), Some(WORKS)));
    assert_eq!(named(&ui, "Linear (Europe)").unwrap().id, linear, "the same server");
}

/// One with no sign-in first, which says how else it's saved.
#[gpui_kit::test]
fn a_server_with_a_key_is_signed_in_to_from_its_edit_and_then_its_key_is_forgotten(cx: &mut TestAppContext) {
    let script = ToolServerScript { asks: ToolServerAsks::Nothing, ..ToolServerScript::default() };
    let (stack, serving) = (Stack::new(), Serving::with(script));
    let server = stack.client().tool_server_add(&"t".into(), "Linear", &serving.url(), with_key()).unwrap().id;
    let mut ui = in_servers(cx, &stack);
    ui.wait("its row", |ui| ui.has(more(&server)));
    edit(&mut ui, &server);
    choose(&mut ui, SIGN_IN_WAY);
    let until = "Your browser opens on its own page. Nothing changes until you've signed in.";
    assert_eq!(label_of(&mut ui, "mcp-way-says"), until);
    assert_eq!(says(&mut ui), "Once you've signed in, running agents have the change at once.");

    ui.click("mcp-panel-go");
    let no_sign_in = "This server has no sign-in. Anyone who can reach it can use it. To save it, choose None, or Key if \
                      it gave you one.";
    assert_eq!(refused(&mut ui), no_sign_in);
    assert_eq!(label_of(&mut ui, "mcp-way"), "Authentication: Sign in", "it doesn't move by itself");
    serving.stand_in.change(|script| script.asks = ToolServerScript::default().asks);
    ui.click("mcp-panel-go");
    let page = page_opened(&mut ui, None);
    assert_eq!(kept(&stack, KEY), 1, "until it's approved");
    serving.sent_back_to(&page).follow();

    ui.wait("its sign-in", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
    assert_eq!(kept(&stack, KEY), 0);
}

#[gpui_kit::test]
fn a_url_that_looks_like_it_has_a_key_in_it_says_so_and_stops_nothing(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), open_to_anyone());
    let mut ui = in_servers(cx, &stack);
    let keyed = format!("{}?searchApiKey=made-up-0123456789", serving.url());
    let warning = "This looks like it has a key in it. Every app on this hub can see the URL. If the server takes a key \
                   in a header, use Key.";
    fill_in(&mut ui, "Search", &serving.url());
    assert_eq!(label_of(&mut ui, "mcp-url-says"), "The server's own page has it.");

    type_in(&mut ui, "mcp-url", &keyed);

    assert_eq!(label_of(&mut ui, "mcp-url-says"), warning);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    ui.wait("it to be added", |ui| named(ui, "Search").is_some() && !ui.has("mcp-panel"));
    let search = named(&ui, "Search").unwrap();
    edit(&mut ui, &search.id);
    assert_eq!(label_of(&mut ui, "mcp-url-says"), warning);
    save(&mut ui);
    assert_eq!(url_of(&ui, "Search"), keyed);
}
