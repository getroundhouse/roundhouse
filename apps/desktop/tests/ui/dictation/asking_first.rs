//! The composer's microphone while dictation asks first, as it does until the person chooses: it offers to turn
//! dictation on, which downloads the model, or to hide the microphone, which turns it off. However its card goes, the
//! composer has the keys again.

use desktop::dictating::STATUS;
use desktop::workspace_pane::{HIDE, MICROPHONE, TURN_ON};
use dictation::fake::FakeEngine;
use fetching::serving::{Reply, serve};
use gpui_kit::TestAppContext;

use super::support::{SCRIPT, says, type_keys, use_model};
use crate::settings::open_settings;
use crate::support::made_up_model::made_up;
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn the_microphone_offers_dictation_and_turning_it_on_there_downloads_the_model(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let (files, bodies) = made_up(&["coreml/vocab.json"], 1000);
    let server = serve(vec![(files.files[0].path.to_string(), vec![Reply::file(bodies[0].clone())])]).unwrap();
    let models = tempfile::tempdir().unwrap();
    let (go, wait) = flume::bounded::<()>(1);
    use_model(&mut ui, models.path(), files, &server.base, move |_| {
        let _ = wait.recv();
        Ok(FakeEngine::new(SCRIPT))
    });

    ui.click(MICROPHONE);
    assert!(ui.has(TURN_ON) && !ui.has(STATUS), "it only offers, until the person says so");
    ui.click(TURN_ON);
    says(&mut ui, "Preparing the model.");
    assert!(files.is_here(models.path()), "{:?}", server.asked());

    go.send(()).unwrap();
    ui.wait("the microphone to dictate", |ui| !ui.has(STATUS) && ui.shows_label(MICROPHONE, "Dictate"));
    type_keys(&mut ui, "a", "a");
    assert_eq!(ui.composer_text(), "a", "the card went, and the composer has the keys");
    open_settings(&mut ui);
    assert!(ui.shows_label("settings-dictation", "Dictation: On"));
}

#[gpui_kit::test]
fn hiding_the_microphone_turns_dictation_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let (files, _) = made_up(&["coreml/vocab.json"], 1000);
    let models = tempfile::tempdir().unwrap();
    use_model(&mut ui, models.path(), files, "http://127.0.0.1:9", |_| Ok(FakeEngine::new(SCRIPT)));

    ui.click(MICROPHONE);
    ui.click(HIDE);

    assert!(!ui.has(MICROPHONE));
    assert!(!ui.has(HIDE), "the popover went with it");
    type_keys(&mut ui, "a", "a");
    assert_eq!(ui.composer_text(), "a", "the composer has the keys");
    open_settings(&mut ui);
    assert!(ui.shows_label("settings-dictation", "Dictation: Off"));
}

#[gpui_kit::test]
fn settings_opened_from_the_microphones_card_gives_the_keys_back_to_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let (files, _) = made_up(&["coreml/vocab.json"], 1000);
    let models = tempfile::tempdir().unwrap();
    use_model(&mut ui, models.path(), files, "http://127.0.0.1:9", |_| Ok(FakeEngine::new(SCRIPT)));

    ui.click(MICROPHONE);
    ui.press("cmd-,");
    ui.wait("Settings", |ui| ui.has("settings-back"));
    ui.press("escape");
    ui.wait("the workspace", |ui| ui.has(MICROPHONE));

    assert!(!ui.has(TURN_ON), "the card went with the composer");
    type_keys(&mut ui, "a", "a");
    assert_eq!(ui.composer_text(), "a", "what had the keys isn't drawn, so the composer has them");
}
