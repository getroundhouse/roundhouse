//! Recording, updating, applying upstream and dropping individual patches.

use std::fs;

use kit_sync::{Error, NOTICE, PatchName, Patched};

use crate::fixture::{Stand, git, lines, noticed_rs, write};

#[test]
fn each_patch_is_kept_separate_without_notices_and_applies_alone_upstream() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first")]))]);
    assert_eq!(stand.patch("first").unwrap(), Patched::Added);
    assert_eq!(stand.patch("first").unwrap(), Patched::Already);

    // A second patch changes the same file further down, and another file too.
    write(
        &stand.copy,
        &[
            ("crates/base/src/lib.rs", &noticed_rs(&lines(12, &[(2, "first"), (11, "second")]))),
            ("crates/kit/src/lib.rs", &lines(12, &[(4, "the kit's")])),
        ],
    );
    assert_eq!(stand.patch("second.patch").unwrap(), Patched::Added);

    let first = stand.read_patch("first");
    let second = stand.read_patch("second");
    assert!(!first.contains(NOTICE) && !second.contains(NOTICE));
    assert!(!first.contains("second") && !second.contains("+first\n"));
    assert_eq!(stand.read("crates/kit/src/lib.rs"), noticed_rs(&lines(12, &[(4, "the kit's")])));
    stand.verify("v1", &stand.first).unwrap();

    // Offering only `second` to a clone of upstream leaves `first` out.
    let fork = stand.work.join("fork");
    git(&stand.work, &["clone", "-q", stand.upstream.to_str().unwrap(), fork.to_str().unwrap()]);
    git(&fork, &["apply", "-3", stand.patches.join("second.patch").to_str().unwrap()]);
    assert_eq!(fs::read_to_string(fork.join("crates/base/src/lib.rs")).unwrap(), lines(12, &[(11, "second")]));
    assert_eq!(fs::read_to_string(fork.join("crates/kit/src/lib.rs")).unwrap(), lines(12, &[(4, "the kit's")]));
}

#[test]
fn updating_an_earlier_patch_refreshes_the_patches_after_it() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first")]))]);
    stand.patch("first").unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first"), (11, "second")]))]);
    stand.patch("second").unwrap();

    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first\ngrown\nmore"), (11, "second")]))]);
    assert_eq!(stand.patch("first").unwrap(), Patched::Updated);

    assert!(stand.read_patch("first").contains("+grown\n"));
    assert!(!stand.read_patch("second").contains("+grown\n"));
    assert!(stand.read_patch("second").contains("+second\n"));
    stand.verify("v1", &stand.first).unwrap();
}

#[test]
fn dropping_a_patch_takes_it_out_of_the_copy_and_removes_stale_notices() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(
        &stand.copy,
        &[
            ("crates/base/src/lib.rs", &lines(12, &[(2, "first")])),
            ("crates/kit/src/lib.rs", &lines(12, &[(3, "only first")])),
            ("crates/base/src/added.rs", "added\n"),
        ],
    );
    stand.patch("first").unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first"), (11, "second")]))]);
    stand.patch("second").unwrap();

    stand.drop_patch("first").unwrap();

    assert!(!stand.patches.join("first.patch").exists());
    assert!(!stand.copy.join("crates/base/src/added.rs").exists());
    assert_eq!(stand.read("crates/kit/src/lib.rs"), lines(12, &[]), "notice removed with the change");
    assert_eq!(stand.read("crates/base/src/lib.rs"), noticed_rs(&lines(12, &[(11, "second")])));
    assert_eq!(stand.made().patches, [PatchName::parse("second").unwrap()]);
    stand.verify("v1", &stand.first).unwrap();

    stand.drop_patch("second").unwrap();
    assert!(!stand.patches.exists(), "empty patches folder removed");
    assert_eq!(stand.read("crates/base/src/lib.rs"), lines(12, &[]));
    stand.verify("v1", &stand.first).unwrap();
}

#[test]
fn dropping_a_patch_another_depends_on_is_refused() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(5, "from first")]))]);
    stand.patch("first").unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(5, "built on first")]))]);
    stand.patch("second").unwrap();

    let refused = stand.drop_patch("first");

    let Err(Error::PatchDependedOn { dropped, by }) = refused else { panic!("{refused:?}") };
    assert_eq!((dropped.to_string(), by.to_string()), ("first".to_string(), "second".to_string()));
    stand.verify("v1", &stand.first).unwrap();
}
