//! The test that draws every design.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;

#[test]
fn every_design_is_drawn() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut canvas = super::canvas::Canvas::new(&text_system);
    super::lists::draw(&mut canvas);
    super::chats::draw(&mut canvas);
    super::changes::draw(&mut canvas);
    super::more::draw(&mut canvas);
    super::menus::draw(&mut canvas);
    super::settings::draw(&mut canvas);
    super::git_settings::draw(&mut canvas);
}
