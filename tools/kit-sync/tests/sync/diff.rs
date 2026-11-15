//! Our changes, as a patch to offer upstream.

use kit_sync::{Git, NOTICE};

use crate::fixture::{Stand, lines, write};

#[test]
fn diff_is_our_changes_from_the_root_of_upstreams_repository_without_the_notices() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    stand.commit_root();
    write(
        &stand.copy,
        &[
            ("crates/base/src/lib.rs", &format!("// {NOTICE}\n{}", lines(12, &[(2, "ours")]))),
            ("crates/kit/src/lib.rs", &format!("// {NOTICE}\n{}", lines(12, &[(4, "the kit's")]))),
        ],
    );
    let git = Git::new(std::env::vars_os());
    let upstream = stand.pin("v1", &stand.first, Vec::new()).upstream;

    let patch = kit_sync::diff(&git, &stand.root, &upstream, &[]).unwrap();
    let one = kit_sync::diff(&git, &stand.root, &upstream, &["crates/kit/src/lib.rs".to_string()]).unwrap();

    assert!(patch.contains("--- a/crates/base/src/lib.rs\n+++ b/crates/base/src/lib.rs\n"), "{patch}");
    assert!(patch.contains("-line 2\n+ours\n"), "{patch}");
    assert!(!patch.contains(NOTICE), "{patch}");
    assert_eq!(patch.matches("diff --git").count(), 2, "{patch}");
    assert_eq!(one.matches("diff --git").count(), 1, "{one}");
    assert!(one.contains("+the kit's\n"), "{one}");
}
