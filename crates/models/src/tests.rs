//! Downloading a model's files from a server on this computer, standing in for Hugging Face, with files made up for
//! the test.

#![cfg(feature = "download")]

use std::ops::ControlFlow;
use std::path::Path;

use fetching::Fetched;
use fetching::serving::{Reply, serve};
use ring::digest::{SHA256, digest};

use super::{ModelFile, ModelFiles, Progress, bytes_on_disk, delete};

/// A file of a model, made up: `bytes` of a pattern, with its SHA-256.
fn made_up(path: &'static str, bytes: usize) -> (ModelFile, Vec<u8>) {
    let body: Vec<u8> = (0..bytes).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let sha256 = digest(&SHA256, &body).as_ref().try_into().unwrap();
    (ModelFile { path, size_bytes: body.len() as u64, sha256 }, body)
}

fn model(revision: &'static str, files: Vec<ModelFile>) -> ModelFiles {
    ModelFiles { repository: "test", revision, files: Box::leak(files.into_boxed_slice()) }
}

/// Downloads `model` into `model_dir` from `from`, keeping what it says of its progress.
fn download(model: &ModelFiles, model_dir: &Path, from: &str) -> (Fetched, Vec<Progress>) {
    let mut told = Vec::new();
    let fetched = model.download(model_dir, from, &mut |progress| {
        told.push(progress);
        ControlFlow::Continue(())
    });
    (fetched.unwrap(), told)
}

#[test]
fn each_missing_file_is_downloaded_and_put_in_place_and_those_there_are_left() {
    let (here, here_body) = made_up("coreml/vocab.json", 1000);
    let (missing, missing_body) = made_up("coreml/encoder.mlmodelc/weights/weight.bin", 100_000);
    let model = model("one", vec![here, missing]);
    let model_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(model_dir.path().join("one/coreml")).unwrap();
    std::fs::write(model_dir.path().join("one").join(here.path), &here_body).unwrap();
    let server = serve(vec![(missing.path.to_string(), vec![Reply::file(missing_body.clone())])]).unwrap();

    let (fetched, told) = download(&model, model_dir.path(), &server.base);

    assert_eq!(fetched, Fetched::Done);
    assert_eq!(told.first(), Some(&Progress { done_bytes: 1000, total_bytes: 101_000 }));
    assert_eq!(told.last(), Some(&Progress { done_bytes: 101_000, total_bytes: 101_000 }));
    assert_eq!(server.asked(), [missing.path], "only what's missing is asked for");
    assert_eq!(std::fs::read(model.dir(model_dir.path()).join(missing.path)).unwrap(), missing_body);
    assert!(model.is_here(model_dir.path()));
}

#[test]
fn a_download_stopped_once_it_has_the_lock_asks_for_nothing() {
    let (file, body) = made_up("coreml/vocab.json", 1000);
    let server = serve(vec![(file.path.to_string(), vec![Reply::file(body)])]).unwrap();
    let model_dir = tempfile::tempdir().unwrap();

    let fetched = model("one", vec![file]).download(model_dir.path(), &server.base, &mut |_| ControlFlow::Break(()));

    assert_eq!(fetched.unwrap(), Fetched::Stopped);
    assert!(server.asked().is_empty());
}

#[test]
fn once_its_all_here_other_revisions_go_and_the_lock_stays() {
    let (file, body) = made_up("coreml/vocab.json", 1000);
    let server = serve(vec![(file.path.to_string(), vec![Reply::file(body)])]).unwrap();
    let model_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(model_dir.path().join("older/coreml")).unwrap();
    std::fs::write(model_dir.path().join("older/coreml/vocab.json.part"), b"older").unwrap();

    download(&model("newer", vec![file]), model_dir.path(), &server.base);

    let mut left: Vec<_> = std::fs::read_dir(model_dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    left.sort();
    assert_eq!(left, [".lock", "newer"]);
}

#[test]
fn deleting_removes_every_revision_and_leaves_the_folder_and_its_lock() {
    let model_dir = tempfile::tempdir().unwrap();
    for revision in ["one", "two"] {
        std::fs::create_dir_all(model_dir.path().join(revision).join("coreml")).unwrap();
        std::fs::write(model_dir.path().join(revision).join("coreml/vocab.json"), b"words").unwrap();
    }
    std::fs::write(model_dir.path().join(".lock"), b"").unwrap();

    delete(model_dir.path()).unwrap();

    let left: Vec<_> = std::fs::read_dir(model_dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, [".lock"]);
    assert_eq!(bytes_on_disk(model_dir.path()), 0);
}

/// The mark is an extended attribute of the folder, which Time Machine reads. The temporary folder it's in is left out
/// of backups already, so asking Time Machine would say so either way.
#[cfg(target_os = "macos")]
#[test]
fn the_folder_is_kept_out_of_backups() {
    let (file, body) = made_up("coreml/vocab.json", 1000);
    let server = serve(vec![(file.path.to_string(), vec![Reply::file(body)])]).unwrap();
    let model_dir = tempfile::tempdir().unwrap();
    let excluded = || {
        let mark = ["-p", "com.apple.metadata:com_apple_backup_excludeItem"];
        std::process::Command::new("xattr").args(mark).arg(model_dir.path()).output().unwrap().status.success()
    };
    assert!(!excluded());

    download(&model("one", vec![file]), model_dir.path(), &server.base);

    assert!(excluded());
}

/// A disk too full to save a file says so, naming the device.
#[test]
fn a_full_disk_says_so() {
    let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
    assert_eq!(super::download::not_saved(&full, "this Mac"), "there isn't room for it on this Mac.");
}
