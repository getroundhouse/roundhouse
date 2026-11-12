use super::*;

fn ids(session: &str, id: &str) -> (FileFor, AttachmentId) {
    (FileFor::Session(SessionId::from(session)), AttachmentId::from(id))
}

#[tokio::test]
async fn bytes_come_back_until_theyre_removed() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().to_path_buf());
    let (session, id) = ids("s-1", "a1");

    files.write(&session, &id, b"one".to_vec()).await.unwrap();
    assert_eq!(files.read(&session, &id).await.unwrap().as_deref(), Some(&b"one"[..]));
    files.remove(&session, &id).await.unwrap();

    assert_eq!(files.read(&session, &id).await.unwrap(), None);
    files.remove(&session, &id).await.unwrap();
    assert_eq!(std::fs::read_dir(dir.path().join("s-1")).unwrap().count(), 0, "no partial file is left behind");
}

#[tokio::test]
async fn the_new_workspace_dialogs_files_are_kept_apart_from_every_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().to_path_buf());
    let (session, id) = ids("s-1", "a1");

    files.write(&FileFor::NewWorkspace, &id, b"dialog".to_vec()).await.unwrap();
    files.write(&session, &id, b"chat".to_vec()).await.unwrap();

    assert_eq!(files.read(&FileFor::NewWorkspace, &id).await.unwrap().as_deref(), Some(&b"dialog"[..]));
    assert_eq!(files.read(&session, &id).await.unwrap().as_deref(), Some(&b"chat"[..]));
    assert!(dir.path().join(WORKSPACE_DRAFT_DIR).join("a1").exists());
}

#[tokio::test]
async fn an_id_that_could_name_another_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().join("attachments"));

    for (session, id) in [("s", "../hub"), ("..", "a1"), ("s", "A1"), ("s", ""), ("s", "a.b"), ("s/t", "a1")] {
        let (session, id) = ids(session, id);
        let refused = files.write(&session, &id, b"x".to_vec()).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::InvalidRequest, "{session:?} {id}");
    }
    let refused = files.write(&FileFor::NewWorkspace, &"../hub".into(), b"x".to_vec()).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidRequest, "the dialog's too");
    assert!(!dir.path().join("attachments").exists(), "nothing was written");
}
