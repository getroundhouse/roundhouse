//! Turning dictation on in Settings: the model downloaded and prepared, the microphone dimmed until it's ready, what
//! went wrong with Retry, and turning it off, which stops a download and keeps what came. With a made-up model, from
//! a server on this computer, and a stand-in for the model.

use std::sync::atomic::{AtomicUsize, Ordering};

use desktop::dictating::RETRY;
use desktop::workspace_pane::MICROPHONE;
use dictation::EngineError;
use dictation::fake::FakeEngine;
use fetching::serving::{Reply, serve};
use gpui_kit::TestAppContext;

use super::support::{SCRIPT, choose, says, status, use_model};
use crate::settings::open_settings;
use crate::support::made_up_model::{made_up, put};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn turning_it_on_downloads_and_prepares_the_model_and_the_microphone_dictates_once_its_ready(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let (files, bodies) = made_up(&["coreml/vocab.json", "coreml/encoder.mlmodelc/weights/weight.bin"], 50_000);
    let replies =
        files.files.iter().zip(&bodies).map(|(file, body)| (file.path.to_string(), vec![Reply::file(body.clone())]));
    let server = serve(replies.collect()).unwrap();
    let models = tempfile::tempdir().unwrap();
    let (go, wait) = flume::bounded::<()>(1);
    let (loading, loaded_from) = flume::bounded(1);
    use_model(&mut ui, models.path(), files, &server.base, move |dir| {
        let _ = loading.send(dir.to_path_buf());
        let _ = wait.recv();
        Ok(FakeEngine::new(SCRIPT))
    });

    open_settings(&mut ui);
    assert_eq!(status(&mut ui), None, "nothing to say while it asks first, with no model here");
    choose(&mut ui, "On");
    says(&mut ui, "Preparing the model.");
    assert!(files.is_here(models.path()), "{:?}", server.asked());
    assert_eq!(loaded_from.recv().unwrap(), files.dir(models.path()), "it loads what was downloaded");
    ui.press("escape");
    assert!(ui.shows_label(MICROPHONE, "Dictation isn't ready"), "dimmed until it's ready");

    go.send(()).unwrap();
    ui.wait("the microphone to dictate", |ui| ui.shows_label(MICROPHONE, "Dictate"));
    open_settings(&mut ui);
    says(&mut ui, "Ready. The model is on this Mac, 0.0 GB.");
    choose(&mut ui, "Off");
    says(&mut ui, "The model is still on this Mac");
    ui.press("escape");
    assert!(!ui.has(MICROPHONE), "no microphone while it's off");
}

#[gpui_kit::test]
fn a_download_cut_short_says_so_and_retry_goes_on_from_what_came(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (files, bodies) = made_up(&["coreml/vocab.json"], 200_000);
    let body = &bodies[0];
    let replies = vec![Reply::cut_off(body[..80_000].to_vec(), body.len()), Reply::partial(body[80_000..].to_vec())];
    let server = serve(vec![(files.files[0].path.to_string(), replies)]).unwrap();
    let models = tempfile::tempdir().unwrap();
    use_model(&mut ui, models.path(), files, &server.base, |_| Ok(FakeEngine::new(SCRIPT)));

    open_settings(&mut ui);
    choose(&mut ui, "On");
    says(&mut ui, "Couldn't download the model: the connection was lost.");
    ui.click(RETRY);
    says(&mut ui, "Ready.");

    assert!(server.requests()[1].to_lowercase().contains("range: bytes=80000-"), "{:?}", server.requests());
    assert!(files.is_here(models.path()));
}

#[gpui_kit::test]
fn a_model_that_wont_load_says_why_and_retry_tries_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (files, bodies) = made_up(&["coreml/vocab.json"], 100);
    let models = tempfile::tempdir().unwrap();
    put(models.path(), &files, &files.files[0], &bodies[0]);
    let tries = AtomicUsize::new(0);
    use_model(&mut ui, models.path(), files, "http://127.0.0.1:9", move |_| {
        match tries.fetch_add(1, Ordering::SeqCst) {
            0 => Err(EngineError("its files are damaged".to_string())),
            _ => Ok(FakeEngine::new(SCRIPT)),
        }
    });

    open_settings(&mut ui);
    choose(&mut ui, "On");
    says(&mut ui, "Couldn't prepare the model: its files are damaged.");
    ui.click(RETRY);
    says(&mut ui, "Ready.");
}

#[gpui_kit::test]
fn turning_it_off_stops_the_download_and_keeps_what_came(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (files, bodies) = made_up(&["coreml/vocab.json"], 1_000_000);
    let (held, release) = Reply::file(bodies[0].clone()).held_after(100_000);
    let server = serve(vec![(files.files[0].path.to_string(), vec![held])]).unwrap();
    let models = tempfile::tempdir().unwrap();
    use_model(&mut ui, models.path(), files, &server.base, |_| Ok(FakeEngine::new(SCRIPT)));
    let part = files.dir(models.path()).join("coreml/vocab.json.part");
    let came = || std::fs::metadata(&part).map_or(0, |found| found.len());

    open_settings(&mut ui);
    choose(&mut ui, "On");
    says(&mut ui, "Downloading the model.");
    ui.wait("what was sent to be written", |_| came() == 100_000);
    choose(&mut ui, "Off");
    says(&mut ui, "The model is still on this Mac");
    // The download hears that it's stopped at its next read.
    release.now();
    let lock = std::fs::File::open(models.path().join(".lock")).unwrap();
    ui.wait("the download to let go of the model's folder", |_| lock.try_lock().is_ok());

    assert!(came() >= 100_000 && came() < 1_000_000, "kept what came, and no more: {}", came());
    assert!(!files.is_here(models.path()));
}
