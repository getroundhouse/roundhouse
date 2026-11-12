use super::*;

fn ids(session: &str, id: &str) -> (SessionId, AttachmentId) {
    (SessionId::from(session), AttachmentId::from(id))
}

#[tokio::test]
async fn bytes_come_back_until_theyre_removed() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().to_path_buf());
    let (session, id) = ids("s-1", "a1");

    for of in [FilesOf::Session(&session), FilesOf::NewWorkspace] {
        files.write(of, &id, b"one".to_vec()).await.unwrap();
        assert_eq!(files.read(of, &id).await.unwrap().as_deref(), Some(&b"one"[..]), "{of:?}");
        files.remove(of, &id).await.unwrap();

        assert_eq!(files.read(of, &id).await.unwrap(), None, "{of:?}");
        files.remove(of, &id).await.unwrap();
    }
    for dir in ["s-1", NEW_WORKSPACE_DRAFT_DIR].map(|name| dir.path().join(name)) {
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "no partial file is left behind in {dir:?}");
    }
}

/// The same ID can name a file of a session's and one of the dialog's, as they're kept apart.
#[tokio::test]
async fn the_new_workspace_dialogs_files_are_kept_apart_from_every_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().to_path_buf());
    let (session, id) = ids("s-1", "a1");

    files.write(FilesOf::Session(&session), &id, b"the chat's".to_vec()).await.unwrap();
    files.write(FilesOf::NewWorkspace, &id, b"the dialog's".to_vec()).await.unwrap();

    let read = |of| files.read(of, &id);
    assert_eq!(read(FilesOf::Session(&session)).await.unwrap().as_deref(), Some(&b"the chat's"[..]));
    assert_eq!(read(FilesOf::NewWorkspace).await.unwrap().as_deref(), Some(&b"the dialog's"[..]));
    files.remove_session(&session).await.unwrap();
    assert_eq!(read(FilesOf::NewWorkspace).await.unwrap().as_deref(), Some(&b"the dialog's"[..]));
}

#[tokio::test]
async fn an_id_that_could_name_another_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let files = AttachmentFiles::new(dir.path().join("attachments"));

    for (session, id) in [("s", "../hub"), ("..", "a1"), ("s", "A1"), ("s", ""), ("s", "a.b"), ("s/t", "a1")] {
        let (session, id) = ids(session, id);
        let refused = files.write(FilesOf::Session(&session), &id, b"x".to_vec()).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::InvalidRequest, "{session} {id}");
    }
    for id in ["../hub", "A1", "", "a.b"].map(AttachmentId::from) {
        let refused = files.write(FilesOf::NewWorkspace, &id, b"x".to_vec()).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::InvalidRequest, "{id}");
    }
    assert!(!dir.path().join("attachments").exists(), "nothing was written");
}
