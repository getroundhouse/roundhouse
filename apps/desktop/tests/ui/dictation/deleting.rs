//! Deleting the model from Settings: dictation turns off first if it's on, the files go, and turning it on while they
//! go downloads them again only once they've gone. With a made-up model, from a server on this computer.

use desktop::dictating::DELETE;
use dictation::fake::FakeEngine;
use fetching::FolderLock;
use fetching::serving::{Reply, serve};
use gpui_kit::TestAppContext;

use super::support::{SCRIPT, choose, says, status, use_model};
use crate::settings::open_voice_settings;
use crate::support::made_up_model::{made_up, put};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn deleting_the_model_while_its_on_turns_dictation_off_and_takes_its_files(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (files, bodies) = made_up(&["coreml/vocab.json"], 1000);
    let models = tempfile::tempdir().unwrap();
    put(models.path(), &files, &files.files[0], &bodies[0]);
    use_model(&mut ui, models.path(), files, "http://127.0.0.1:9", |_| Ok(FakeEngine::new(SCRIPT)));

    open_voice_settings(&mut ui);
    choose(&mut ui, "On");
    says(&mut ui, "Ready.");
    ui.click(DELETE);
    ui.wait("the model to be gone", |ui| status(ui).is_none());

    assert!(ui.shows_label("settings-dictation", "Dictation: Off"));
    assert!(!files.dir(models.path()).exists());
}

#[gpui_kit::test]
fn turning_it_on_while_the_model_is_deleted_downloads_it_again_once_its_gone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (files, bodies) = made_up(&["coreml/vocab.json"], 1000);
    let server = serve(vec![(files.files[0].path.to_string(), vec![Reply::file(bodies[0].clone())])]).unwrap();
    let models = tempfile::tempdir().unwrap();
    put(models.path(), &files, &files.files[0], &bodies[0]);
    use_model(&mut ui, models.path(), files, &server.base, |_| Ok(FakeEngine::new(SCRIPT)));
    open_voice_settings(&mut ui);
    choose(&mut ui, "On");
    says(&mut ui, "Ready.");

    // Holds the model's folder, so deleting waits.
    let held = FolderLock::take(models.path()).unwrap();
    ui.click(DELETE);
    says(&mut ui, "Deleting the model.");
    choose(&mut ui, "On");
    says(&mut ui, "Deleting the model.");
    drop(held);
    says(&mut ui, "Ready.");

    assert_eq!(server.asked(), [files.files[0].path], "downloaded again, once the files had gone");
    assert!(files.is_here(models.path()));
}
