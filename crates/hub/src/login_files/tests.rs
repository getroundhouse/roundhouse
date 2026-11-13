use std::path::{Path, PathBuf};

use protocol::client::ErrorCode;

use super::*;

/// A secret of a kind no login has yet, of two parts with a byte between them that no text has, so a file of one is
/// read back only by its own kind, and not as text.
#[derive(Debug, PartialEq, Eq)]
struct TwoParts(String, String);

impl LoginSecret for TwoParts {
    const DIR: &'static str = "two-parts";

    fn to_bytes(&self) -> Vec<u8> {
        [self.0.as_bytes(), &[0xff], self.1.as_bytes()].concat()
    }

    fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        let at = bytes.iter().position(|byte| *byte == 0xff)?;
        let (first, second) = (&bytes[..at], &bytes[at + 1..]);
        Some(Self(String::from_utf8(first.to_vec()).ok()?, String::from_utf8(second.to_vec()).ok()?))
    }
}

fn two_parts(first: &str) -> TwoParts {
    TwoParts(first.to_string(), "a-refresh-token".to_string())
}

/// Where the files of the kind [`TwoParts`] are kept, in the data directory `data`.
fn dir_in(data: &Path) -> PathBuf {
    data.join(TwoParts::DIR)
}

/// Keeps `secret` as a login whose row is written without fail, and answers with its file.
fn connected(files: &Locked<'_, TwoParts>, secret: &TwoParts) -> LoginFile {
    files.connect(secret, |file| Ok((file.clone(), None))).unwrap()
}

/// Writes `bytes` as `file` in the directory of [`TwoParts`] in `data`, as something other than the hub would.
fn write_by_hand(data: &Path, file: &LoginFile, bytes: &str) {
    std::fs::create_dir_all(dir_in(data)).unwrap();
    std::fs::write(dir_in(data).join(file.as_str()), bytes).unwrap();
}

/// The name of everything in the directory of [`TwoParts`] in `data`, sorted.
fn names_in(data: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(dir_in(data)).unwrap();
    let mut names: Vec<String> = entries.map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    names
}

/// What a row's write fails with, as a full disk would, which no file's failure gives.
fn refused() -> ApiError {
    ApiError::new(ErrorCode::Conflict, "refused")
}

#[test]
fn a_secret_of_another_kind_is_read_back_whole_from_its_own_directory_and_a_file_that_isnt_one_holds_none() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let not_one = LoginFile::new();
    write_by_hand(dir.path(), &not_one, "a-token-of-another-kind");
    let files = files.lock();

    let kept = connected(&files, &two_parts("an-access-token"));

    assert_eq!(files.read(&kept).unwrap(), Some(two_parts("an-access-token")));
    assert_eq!(files.read(&not_one).unwrap(), None);
    assert!(names_in(dir.path()).contains(&kept.as_str().to_string()), "it's in its kind's directory");
}

#[test]
fn connecting_keeps_the_secret_in_a_private_file_before_its_row_names_it_and_then_lets_go_of_the_one_replaced() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let files = files.lock();
    let old = connected(&files, &two_parts("old"));
    let mut held_as_its_row_was_written = None;

    let new = files
        .connect(&two_parts("new"), |file| {
            held_as_its_row_was_written = Some(files.read(file).unwrap());
            Ok((file.clone(), Some(old.clone())))
        })
        .unwrap();

    assert_eq!(held_as_its_row_was_written, Some(Some(two_parts("new"))));
    assert_eq!(names_in(dir.path()), [new.as_str()], "the replaced file is gone");
    let mode = std::fs::metadata(dir_in(dir.path()).join(new.as_str())).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let dir_mode = std::fs::metadata(dir_in(dir.path())).unwrap().permissions().mode();
    assert_eq!(dir_mode & 0o777, 0o700, "the directory is only the hub's user's too");
}

#[test]
fn a_connect_whose_row_cant_be_written_lets_go_of_its_file_and_keeps_the_one_it_would_have_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let files = files.lock();
    let old = connected(&files, &two_parts("old"));

    let failed = files.connect::<()>(&two_parts("new"), |_| Err(refused()));

    assert_eq!(failed.unwrap_err().code, ErrorCode::Conflict, "it fails as the row's write did");
    assert_eq!(names_in(dir.path()), [old.as_str()]);
}

#[test]
fn a_connect_whose_file_cant_be_written_never_writes_its_row() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir_in(dir.path()), "not a directory").unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let mut written = false;

    let failed = files.lock().connect(&two_parts("new"), |file| {
        written = true;
        Ok((file.clone(), None))
    });

    assert_eq!(failed.unwrap_err().code, ErrorCode::Internal);
    assert!(!written);
}

#[test]
fn disconnecting_removes_the_file_before_its_row_is_cleared_and_even_when_the_row_cant_be() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let files = files.lock();
    let [cleared, refusing] = ["cleared", "refusing"].map(|first| connected(&files, &two_parts(first)));
    let mut there_as_its_row_was_cleared = None;

    let disconnected = files.disconnect(&cleared, || {
        there_as_its_row_was_cleared = Some(dir_in(dir.path()).join(cleared.as_str()).exists());
        Ok("cleared")
    });
    let failed = files.disconnect::<()>(&refusing, || Err(refused()));

    assert_eq!(disconnected.unwrap(), "cleared");
    assert_eq!(there_as_its_row_was_cleared, Some(false));
    assert_eq!(failed.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(names_in(dir.path()), Vec::<String>::new(), "the file of a row that couldn't be cleared is gone too");
}

#[test]
fn the_row_whose_file_holds_a_secret_is_found_past_one_that_cant_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let files = files.lock();
    let [unreadable, other, holding] =
        ["unreadable", "other", "holding"].map(|first| connected(&files, &two_parts(first)));
    // A directory where the file was, which reading fails on even as root, as it wouldn't on a file whose mode changed.
    std::fs::remove_file(dir_in(dir.path()).join(unreadable.as_str())).unwrap();
    std::fs::create_dir(dir_in(dir.path()).join(unreadable.as_str())).unwrap();
    let rows = vec![(1, unreadable), (2, other), (3, holding)];

    let found = files.holding(rows.clone(), &two_parts("holding"));
    let unread = files.holding(rows.clone(), &two_parts("unreadable"));
    let nobodys = files.holding(rows, &two_parts("nobody's"));

    assert_eq!((found, unread, nobodys), (Some(3), None, None));
}

#[test]
fn tidying_clears_each_row_whose_file_holds_none_of_its_kind_and_keeps_only_the_files_of_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let files = files.lock();
    let [kept, unreadable, emptied, unnamed] =
        ["kept", "unreadable", "emptied", "unnamed"].map(|first| connected(&files, &two_parts(first)));
    let (of_another_kind, gone) = (LoginFile::new(), LoginFile::new());
    write_by_hand(dir.path(), &of_another_kind, "a-token-of-another-kind");
    std::fs::write(dir_in(dir.path()).join(emptied.as_str()), "").unwrap();
    std::fs::remove_file(dir_in(dir.path()).join(unreadable.as_str())).unwrap();
    std::fs::create_dir(dir_in(dir.path()).join(unreadable.as_str())).unwrap();
    std::fs::write(dir_in(dir.path()).join("half-written.partial"), "a-half").unwrap();
    let named = vec![
        (1, Some(kept.clone())),
        (2, Some(unreadable.clone())),
        (3, Some(emptied)),
        (4, Some(of_another_kind)),
        (5, Some(gone)),
        (6, None),
    ];
    let mut cleared = Vec::new();

    let tidied = files.tidy(named, |id| {
        cleared.push(*id);
        Ok(())
    });

    assert!(tidied.is_ok());
    assert_eq!(cleared, [3, 4, 5, 6]);
    // The file that can't be read is a directory, which tidying never removes, so only its row is checked, above.
    let mut left = vec![kept.as_str(), unreadable.as_str()];
    left.sort_unstable();
    assert_eq!(names_in(dir.path()), left, "neither {} nor any file but the rows' is left", unnamed.as_str());
}

#[test]
fn tidying_clears_the_rows_even_when_the_files_cant_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir_in(dir.path()), "not a directory").unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());
    let mut cleared = Vec::new();

    let tidied = files.lock().tidy(vec![(1, None)], |id| {
        cleared.push(*id);
        Ok(())
    });

    assert_eq!(tidied.unwrap_err().code, ErrorCode::Internal);
    assert_eq!(cleared, [1]);
}

#[test]
fn tidying_fails_as_a_row_that_cant_be_cleared_does() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());

    let tidied = files.lock().tidy(vec![(1, None)], |_| Err(refused()));

    assert_eq!(tidied.unwrap_err().code, ErrorCode::Conflict);
}

#[test]
fn tidying_a_kind_that_has_never_had_a_file_makes_no_directory() {
    let dir = tempfile::tempdir().unwrap();
    let files = LoginFiles::<TwoParts>::new(dir.path());

    files.lock().tidy(Vec::<(u32, Option<LoginFile>)>::new(), |_| Ok(())).unwrap();

    assert!(!dir_in(dir.path()).exists());
}
