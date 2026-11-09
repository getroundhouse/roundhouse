use super::*;

#[test]
fn a_file_from_disk_is_the_kind_its_bytes_say() {
    let dir = tempfile::tempdir().unwrap();
    let (log, blob) = (dir.path().join("build.png"), dir.path().join("blob"));
    std::fs::write(&log, "error at line 42\n").unwrap();
    std::fs::write(&blob, [0, 1, 2]).unwrap();

    let log = FileToSend::read(&log).unwrap();
    let blob = FileToSend::read(&blob).unwrap();

    assert_eq!((log.attachment.name.as_str(), log.attachment.media_type), ("build.png", MediaType::Text));
    assert_eq!((blob.attachment.media_type, blob.attachment.size_bytes), (MediaType::Other, 3));
    assert_ne!(log.attachment.id, blob.attachment.id);
}

#[test]
fn a_folder_an_empty_file_or_a_missing_one_says_why_it_cant_go() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("notes")).unwrap();
    std::fs::write(dir.path().join("empty.log"), "").unwrap();

    let folder = FileToSend::read(&dir.path().join("notes")).unwrap_err();
    let empty = FileToSend::read(&dir.path().join("empty.log")).unwrap_err();
    let missing = FileToSend::read(&dir.path().join("gone.log")).unwrap_err();

    assert_eq!(folder.to_string(), "couldn't attach notes: a folder can't be attached");
    assert_eq!(empty.to_string(), "couldn't attach empty.log: the file is empty");
    assert_eq!(missing.name, "gone.log");
}

#[test]
fn a_file_over_the_limit_is_refused_without_being_read() {
    let dir = tempfile::tempdir().unwrap();
    let huge = dir.path().join("huge.bin");
    // Sparse, so the test doesn't write 100 MB.
    std::fs::File::create(&huge).unwrap().set_len(MAX_FILE_BYTES + 1).unwrap();

    let refused = FileToSend::read(&huge).unwrap_err();

    assert_eq!(refused.to_string(), "couldn't attach huge.bin: a file can be at most 100 MB");
}
