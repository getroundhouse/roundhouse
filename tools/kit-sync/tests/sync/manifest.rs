//! Upstream's workspace manifest, cut down to the packages the copy keeps.

use kit_sync::Error;

use crate::fixture::{CRATES, Stand, workspace, write};

#[test]
fn cutting_the_members_keeps_the_rest_of_upstreams_manifest() {
    let stand = Stand::new();
    write(&stand.upstream, &[("Cargo.toml", &workspace("=0.3.11", "  # Every package"))]);
    let second = stand.release("v2");

    stand.sync("v2", &second).unwrap();

    let manifest: toml::Table = stand.read("Cargo.toml").parse().unwrap();
    assert_eq!(manifest["workspace"]["resolver"].as_str(), Some("2"));
    let package = manifest["workspace"]["package"].as_table().unwrap();
    assert_eq!((package.len(), package["edition"].as_str()), (1, Some("2024")));
    assert_eq!(manifest["workspace"]["members"].as_array().unwrap(), &CRATES.map(toml::Value::from).to_vec());
}

#[test]
fn members_that_cant_be_cut_alone_are_refused() {
    let stand = Stand::new();
    let manifest = workspace("=0.3.11", "").replace("members = [\n", "members = [ # The crates, then [the rest]\n");
    write(&stand.upstream, &[("Cargo.toml", &manifest)]);
    let second = stand.release("v2");

    let refused = stand.sync("v2", &second);

    assert!(matches!(refused, Err(Error::MembersEntangled)), "{refused:?}");
    assert!(!stand.copy.exists());
}
