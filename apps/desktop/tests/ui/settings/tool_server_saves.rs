//! What the app asks of the hub as a server is added or edited in Settings, and what the panel shows as the hub answers,
//! against a fake hub. Every server and key is plainly made up.

#![cfg(test)]

use client_core::fake_hub::{ToolServerSaveWorld, World};
use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use protocol::client::tool_server_checks::NoToolList;
use protocol::client::tool_server_saves::ToolServerSaved;
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::tool_server;

use super::open_settings;
use super::tool_server_check::buttons;
use super::tool_server_edit::edit;
use super::tool_servers::{KEY, KEY_WAY, choose, fill_in, type_in};
use super::tool_sign_in::refused;
use crate::support::{FakeHub, Ui};

/// The window on a fake hub that holds `world`, with Add's panel filled in, with a key.
pub(super) fn filled_in_on(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("Add server to be on", |ui| ui.on("mcp-add"));
    fill_in(&mut ui, "Runbooks", "https://mcp.example.com/mcp");
    choose(&mut ui, KEY_WAY);
    ui.input(KEY);
    (fake, ui)
}

/// It's refused as any request the hub doesn't know is, which client-core's `refusals` words, and nothing else is
/// asked of it.
#[gpui_kit::test]
fn a_hub_from_before_saves_is_to_be_updated_to_add_a_server(cx: &mut TestAppContext) {
    let mut world = World::default();
    let refuses = ApiError::new(ErrorCode::Unsupported, "a hub from before saves");
    world.tool_server_save = ToolServerSaveWorld::Answers(Err(refuses));
    let asked = world.asked.clone();
    let (_fake, mut ui) = filled_in_on(cx, world);

    ui.click("mcp-panel-go");

    assert_eq!(refused(&mut ui), "Couldn't add it: this hub can't do that until it's updated");
    assert_eq!(buttons(&mut ui), ("Add".to_string(), None));
    let asked_each = ["tool_server_save", "tool_server_check", "tool_server_add"].map(|method| asked.count(method));
    assert_eq!(asked_each, [1, 0, 0]);
}

/// So that running agents are asked to connect to it again. The fake hub saves nothing.
#[gpui_kit::test]
fn an_edit_thats_saved_anyway_says_so_to_the_hub(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.tool_servers = vec![tool_server("runbooks")];
    let refused_key = ToolServerSaved::NotListed { why: NoToolList::KeyRefused { status: 401 } };
    world.tool_server_save = ToolServerSaveWorld::Answers(Ok(refused_key));
    let asked = world.asked.clone();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("its row", |ui| ui.has("mcp-more-runbooks"));

    edit(&mut ui, &"runbooks".into());
    type_in(&mut ui, "mcp-key", KEY);
    ui.click("mcp-panel-go");
    let name = tool_server("runbooks").name;
    assert_eq!(refused(&mut ui), format!("{name} refused this key."));
    ui.click("mcp-anyway");

    ui.wait("the hub to be asked again", |_| asked.count("tool_server_save") == 2);
    let save = asked.last("tool_server_save").unwrap();
    assert_eq!((save["of"]["id"].as_str(), save["when"].as_str()), (Some("runbooks"), Some("anyway")));
}

/// Under the last row of more servers than fit, where the panel's end is at the page's end as it opens.
#[gpui_kit::test]
fn a_panel_thats_longer_with_the_reason_why_has_its_end_in_sight(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.tool_servers = (1..=16).map(|at| tool_server(&format!("server-{at:02}"))).collect();
    let refused_key = ToolServerSaved::NotListed { why: NoToolList::KeyRefused { status: 401 } };
    world.tool_server_save = ToolServerSaveWorld::Answers(Ok(refused_key));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("their rows", |ui| ui.has("mcp-more-server-16"));
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    let ends = |ui: &mut Ui<'_>| (ui.bounds("mcp-panel").bottom(), ui.bounds("settings-body").bottom());
    edit(&mut ui, &"server-16".into());
    ui.bring_the_next_frame();
    let (panel, page) = ends(&mut ui);
    assert!(panel <= page, "as it opens: {panel:?}, {page:?}");
    type_in(&mut ui, "mcp-key", KEY);

    ui.click("mcp-panel-go");
    ui.wait("the hub's answer", |ui| ui.has("mcp-anyway"));
    assert_eq!(buttons(&mut ui), ("Try again".to_string(), Some("Save anyway".to_string())));
    ui.bring_the_next_frame();

    let (panel, page) = ends(&mut ui);
    assert!(panel <= page, "with the reason: {panel:?}, {page:?}");
}

/// The hub saves a server's name alone without asking it anything, which client-core's `is_asked` table has with the
/// rest, so the panel doesn't say it's asked. The fake hub goes on saving it.
#[gpui_kit::test]
fn the_panel_says_the_server_is_asked_only_where_the_hub_asks_it_and_cancel_lets_go_of_the_save(
    cx: &mut TestAppContext,
) {
    let mut world = World::default();
    world.tool_servers = vec![tool_server("runbooks")];
    world.tool_server_save = ToolServerSaveWorld::Asking;
    let asked = world.asked.clone();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("its row", |ui| ui.has("mcp-more-runbooks"));

    edit(&mut ui, &"runbooks".into());
    type_in(&mut ui, "mcp-name", "Northwind");
    ui.click("mcp-panel-go");
    assert!(ui.off("mcp-panel-go") && !ui.has("mcp-asking"));
    // It's sent on a thread of its own, and one let go of before it's sent sends nothing, so the hub has it first.
    crate::support::wait_until("the hub to have the save", || asked.count("tool_server_save") == 1);
    ui.click("mcp-cancel");

    ui.wait("it to be let go of", |_| asked.count("tool_server_save_cancel") == 1);
    assert_eq!(asked.count("tool_server_save"), 1);
}
