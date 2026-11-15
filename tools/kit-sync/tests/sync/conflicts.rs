//! A merge that leaves conflicts, and running it again once they're settled.

use kit_sync::{CopyPath, Error, Synced, Versions};

use crate::fixture::{Stand, lines, workspace, write};

#[test]
fn a_conflict_is_left_marked_and_checked_before_it_counts_as_settled() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    stand.commit_root();
    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "ours")]))]);
    write(
        &stand.upstream,
        &[("Cargo.toml", &workspace("=0.3.12", "")), ("crates/kit/src/lib.rs", &lines(12, &[(5, "theirs")]))],
    );
    let second = stand.release("v2");
    let conflicted = [CopyPath::new("crates/kit/src/lib.rs")];

    assert_eq!(stand.sync("v2", &second).unwrap(), Synced::Unsettled(conflicted.to_vec()));

    let marked = stand.read("crates/kit/src/lib.rs");
    assert!(marked.contains("<<<<<<<") && marked.contains("ours") && marked.contains("theirs"), "{marked}");
    assert_eq!((&stand.made().commit, &stand.made().unsettled[..]), (&second, &conflicted[..]));
    assert_eq!(stand.versions().1, "=0.3.11", "moved only once nothing is left to settle");
    let Err(Error::StillMarked(still)) = stand.sync("v2", &second) else { panic!("a marked conflict counted") };
    assert_eq!(still, conflicted);

    write(&stand.copy, &[("crates/kit/src/lib.rs", &lines(12, &[(5, "settled")]))]);
    let moved = Versions { kit: "=0.1.0".to_string(), gpui: "=0.3.12".to_string() };
    assert_eq!(stand.sync("v2", &second).unwrap(), Synced::Already { moved: Some(moved) });
    assert_eq!(stand.read("crates/kit/src/lib.rs"), lines(12, &[(5, "settled")]));
    assert_eq!(stand.made().unsettled, []);
}
