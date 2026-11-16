//! A sync that stops on a conflicting patch, and continuing once it's settled.

use kit_sync::{CopyPath, Error, PatchName, Synced, Versions};

use crate::fixture::{Stand, lines, noticed_rs, workspace, write};

#[test]
fn a_conflict_names_its_patch_waits_to_be_settled_and_continues_to_later_patches() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "clean before")]))]);
    stand.patch("before").unwrap();
    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "ours")]))]);
    stand.patch("conflicting").unwrap();
    write(&stand.copy, &[("crates/component/src/lib.rs", &lines(12, &[(9, "clean after")]))]);
    stand.patch("after").unwrap();
    stand.commit_root();

    write(
        &stand.upstream,
        &[("Cargo.toml", &workspace("=0.3.12", "")), ("crates/kit/src/lib.rs", &lines(12, &[(5, "theirs")]))],
    );
    let second = stand.release("v2");
    let conflicted = [CopyPath::new("crates/kit/src/lib.rs")];
    let patch = PatchName::parse("conflicting").unwrap();

    assert_eq!(
        stand.sync("v2", &second).unwrap(),
        Synced::Unsettled { patch: patch.clone(), files: conflicted.to_vec() }
    );

    let marked = stand.read("crates/kit/src/lib.rs");
    assert!(marked.contains("<<<<<<<") && marked.contains("ours") && marked.contains("theirs"), "{marked}");
    assert_eq!(stand.versions().1, "=0.3.11", "moved only once nothing is left to settle");
    assert!(matches!(stand.verify("v2", &second), Err(Error::Unsettled { .. })));
    let Err(Error::StillMarked(still)) = stand.sync("v2", &second) else { panic!("a marked conflict counted") };
    assert_eq!(still, conflicted);

    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "settled")]))]);
    let moved = Versions { kit: "=0.1.0".to_string(), gpui: "=0.3.12".to_string() };
    assert_eq!(stand.sync("v2", &second).unwrap(), Synced::Brought { moved: Some(moved), dropped: Vec::new() });
    assert_eq!(stand.read("crates/base/src/lib.rs"), noticed_rs(&lines(12, &[(2, "clean before")])));
    assert_eq!(stand.read("crates/kit/src/lib.rs"), noticed_rs(&lines(12, &[(5, "settled")])));
    assert_eq!(stand.read("crates/component/src/lib.rs"), noticed_rs(&lines(12, &[(9, "clean after")])));
    assert!(stand.read_patch("conflicting").contains("-theirs\n+settled\n"));
    assert_eq!(stand.made().settling, None);
    stand.verify("v2", &second).unwrap();
}

#[test]
fn settling_a_conflict_to_upstreams_version_drops_the_patch() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "ours")]))]);
    stand.patch("ours").unwrap();
    write(&stand.upstream, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "upstream's better way")]))]);
    let second = stand.release("v2");

    assert!(matches!(stand.sync("v2", &second).unwrap(), Synced::Unsettled { .. }));

    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "upstream's better way")]))]);
    let synced = stand.sync("v2", &second).unwrap();
    assert_eq!(synced, Synced::Brought { moved: None, dropped: vec![PatchName::parse("ours").unwrap()] });
    assert!(!stand.patches.join("ours.patch").exists());
    assert_eq!(stand.read("crates/kit/src/lib.rs"), lines(12, &[(5, "upstream's better way")]));
    stand.verify("v2", &second).unwrap();
}
