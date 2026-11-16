//! What's refused before anything of the copy or its patches is lost or left out of step.

use std::fs;

use kit_sync::{CopyPath, Error, Read};

use crate::fixture::{LEFT_OUT, Stand, lines, write};

#[test]
fn a_tag_that_isnt_on_the_pinned_commit_is_refused() {
    let stand = Stand::new();
    write(&stand.upstream, &[("crates/kit/src/lib.rs", &lines(12, &[(1, "moved")]))]);
    let second = stand.release("v2");

    let refused = stand.sync("v2", &stand.first);

    let Err(Error::TagElsewhere { tagged, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(tagged, Some(second));
    assert!(!stand.copy.exists());
}

#[test]
fn a_folder_to_leave_out_that_upstream_no_longer_has_is_refused() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    fs::remove_dir_all(stand.upstream.join(LEFT_OUT)).unwrap();
    let second = stand.release("v2");

    let refused = stand.sync("v2", &second);

    let Err(Error::NothingToLeaveOut(path)) = refused else { panic!("{refused:?}") };
    assert_eq!(path, CopyPath::new(LEFT_OUT));
    assert_eq!(stand.made().commit, stand.first);
}

#[test]
fn files_with_nothing_to_say_where_they_came_from_are_refused() {
    let stand = Stand::new();
    write(&stand.copy, &[("crates/kit/lib.rs", "whose?\n")]);

    let refused = stand.sync("v1", &stand.first);

    let Err(Error::Unrecorded(theirs)) = refused else { panic!("{refused:?}") };
    assert_eq!(theirs, [CopyPath::new("crates/kit/lib.rs")]);
    assert_eq!(stand.read("crates/kit/lib.rs"), "whose?\n");
}

#[test]
fn code_that_reads_a_file_the_copy_doesnt_keep_is_refused() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(
        &stand.upstream,
        &[
            ("crates/base/src/lib.rs", "pub const GUIDE: &str = include_str!(\"../../../docs/guide.md\");\n"),
            ("docs/guide.md", "upstream's guide\n"),
        ],
    );
    let second = stand.release("v2");

    let refused = stand.sync("v2", &second);

    let Err(Error::ReadsUnkept(unread)) = refused else { panic!("{refused:?}") };
    let by = CopyPath::new("crates/base/src/lib.rs");
    assert_eq!(unread, [Read { file: "docs/guide.md".to_string(), by }]);
    assert_eq!((stand.made().commit, stand.read("crates/base/src/lib.rs")), (stand.first.clone(), lines(12, &[])));
}

#[test]
fn a_file_this_repository_ignores_is_refused() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.upstream, &[("crates/kit/dist/bundle.js", "bundled\n")]);
    let second = stand.release("v2");

    let refused = stand.sync("v2", &second);

    let Err(Error::Ignored(ignored)) = refused else { panic!("{refused:?}") };
    assert_eq!(ignored, [CopyPath::new("crates/kit/dist/bundle.js")]);
    assert_eq!(stand.made().commit, stand.first);
}

#[test]
fn unpatched_changes_missing_patches_and_missing_notices_are_refused() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    assert!(matches!(stand.patch("empty"), Err(Error::NothingChanged)));

    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "recorded")]))]);
    stand.patch("recorded").unwrap();

    // An unpatched edit to an untouched file is refused by verify, sync and drop.
    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(3, "loose")]))]);
    assert!(matches!(stand.verify("v1", &stand.first), Err(Error::UnpatchedChanges)));
    assert!(matches!(stand.sync("v1", &stand.first), Err(Error::UnpatchedChanges)));
    assert!(matches!(stand.drop_patch("recorded"), Err(Error::UnpatchedChanges)));
    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[]))]);

    // Removing the notice line from a patched file is refused.
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "recorded")]))]);
    let Err(Error::WrongNotices(wrong)) = stand.verify("v1", &stand.first) else { panic!("missing notice passed") };
    assert_eq!(wrong, [CopyPath::new("crates/base/src/lib.rs")]);
    stand.patch("recorded").unwrap();

    // An unlisted file in the patches folder is refused.
    write(&stand.patches, &[("stray.patch", "stray\n")]);
    let Err(Error::UnlistedPatch(stray)) = stand.verify("v1", &stand.first) else { panic!("unlisted patch passed") };
    assert_eq!(stray, "stray.patch");
    fs::remove_file(stand.patches.join("stray.patch")).unwrap();
    stand.verify("v1", &stand.first).unwrap();
}
