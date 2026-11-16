//! Making the copy, and bringing it and each patch to a release.

use std::fs;

use kit_sync::{Kept, NOTICE, PatchName, Synced, Versions};

use crate::fixture::{CRATES, FILES, LEFT_OUT, LINK, Stand, UNKEPT, kept, lines, noticed_rs, workspace, write};

#[test]
fn a_first_sync_makes_a_copy_of_only_what_it_keeps() {
    let stand = Stand::new();

    let synced = stand.sync("v1", &stand.first).unwrap();

    let moved = Versions { kit: "=0.1.0".to_string(), gpui: "=0.3.11".to_string() };
    assert_eq!(synced, Synced::Brought { moved: Some(moved), dropped: Vec::new() });
    for crate_folder in CRATES {
        assert_eq!(stand.read(&format!("{crate_folder}/src/lib.rs")), lines(12, &[]));
    }
    for path in FILES {
        let upstream_s = if path == "LICENSE-APACHE" { "Apache License\n" } else { "upstream's\n" };
        assert_eq!(stand.read(path), upstream_s, "{path}");
    }
    for path in UNKEPT {
        assert!(!stand.copy.join(path).exists(), "{path}");
    }
    assert!(!stand.copy.join(LEFT_OUT).exists());
    let manifest = stand.read("Cargo.toml");
    assert!(manifest.starts_with(&format!("# {NOTICE}")));
    let manifest: toml::Table = manifest.parse().unwrap();
    assert_eq!(manifest["workspace"]["members"].as_array().unwrap(), &CRATES.map(toml::Value::from).to_vec());
    assert!(manifest["workspace"].get("default-members").is_none());
    assert_eq!(fs::read_link(stand.copy.join(LINK)).unwrap().to_str(), Some("../../LICENSE-APACHE"));
    let made = stand.made();
    assert_eq!((made.release.to_string(), &made.commit, &made.kept), ("v1".to_string(), &stand.first, &kept()));
    assert_eq!((&made.patches[..], &made.settling), (&[][..], &None));
    assert_eq!(stand.versions(), ("=0.1.0".to_string(), "=0.3.11".to_string()));
    stand.verify("v1", &stand.first).unwrap();
}

#[test]
fn a_sync_updates_each_patch_onto_the_release_and_drops_one_now_upstream() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "first")]))]);
    stand.patch("first").unwrap();
    write(
        &stand.copy,
        &[
            ("crates/kit/src/lib.rs", &lines(12, &[(3, "second")])),
            ("crates/base/src/ours.rs", "ours\n"),
            ("target/debug/built", "built\n"),
        ],
    );
    fs::remove_file(stand.copy.join("crates/assets/src/lib.rs")).unwrap();
    stand.patch("second").unwrap();
    write(&stand.copy, &[("crates/component/src/lib.rs", &lines(12, &[(4, "taken upstream")]))]);
    stand.patch("upstreamed").unwrap();
    stand.commit_root();

    write(
        &stand.upstream,
        &[
            ("Cargo.toml", &workspace("=0.3.12", "")),
            ("crates/kit/Cargo.toml", "[package]\nname = \"gpui-kit\"\nversion = \"0.2.0\"\n"),
            ("crates/base/src/lib.rs", &lines(12, &[(10, "upstream's")])),
            ("crates/kit/src/lib.rs", &lines(12, &[(11, "upstream's")])),
            ("crates/component/src/lib.rs", &lines(12, &[(4, "taken upstream")])),
            ("crates/component/src/new.rs", "new\n"),
        ],
    );
    fs::remove_file(stand.upstream.join("crates/component/src/old.rs")).unwrap();
    let second = stand.release("v2");

    let synced = stand.sync("v2", &second).unwrap();

    let moved = Versions { kit: "=0.2.0".to_string(), gpui: "=0.3.12".to_string() };
    assert_eq!(synced, Synced::Brought { moved: Some(moved), dropped: vec![PatchName::parse("upstreamed").unwrap()] });
    assert_eq!(stand.read("crates/base/src/lib.rs"), noticed_rs(&lines(12, &[(2, "first"), (10, "upstream's")])));
    assert_eq!(stand.read("crates/kit/src/lib.rs"), noticed_rs(&lines(12, &[(3, "second"), (11, "upstream's")])));
    assert_eq!(stand.read("crates/base/src/ours.rs"), noticed_rs("ours\n"));
    assert_eq!(stand.read("crates/component/src/lib.rs"), lines(12, &[(4, "taken upstream")]));
    assert!(!stand.patches.join("upstreamed.patch").exists());
    assert!(stand.read_patch("first").contains("upstream's") || stand.read_patch("first").contains("+first\n"));
    assert!(!stand.read_patch("first").contains(NOTICE));
    assert!(!stand.read_patch("second").contains(NOTICE));
    assert!(!stand.copy.join("crates/assets/src/lib.rs").exists(), "we deleted it, and upstream didn't");
    assert_eq!(stand.read("crates/component/src/new.rs"), "new\n");
    assert!(!stand.copy.join("crates/component/src/old.rs").exists());
    assert_eq!(stand.read("target/debug/built"), "built\n", "what's built isn't the copy's");
    assert_eq!(fs::read_link(stand.copy.join(LINK)).unwrap().to_str(), Some("../../LICENSE-APACHE"));
    assert_eq!(stand.read("LICENSE-APACHE"), "Apache License\n");
    assert_eq!(stand.made().patches, [PatchName::parse("first").unwrap(), PatchName::parse("second").unwrap()]);
    stand.verify("v2", &second).unwrap();
}

#[test]
fn a_link_upstream_makes_a_file_is_written_as_a_file_rather_than_through_the_link() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    stand.commit_root();
    write(&stand.upstream, &[(LINK, "The macros' own license\n")]);
    let second = stand.release("v2");

    stand.sync("v2", &second).unwrap();

    assert!(!fs::symlink_metadata(stand.copy.join(LINK)).unwrap().is_symlink());
    assert_eq!(stand.read(LINK), "The macros' own license\n");
    assert_eq!(stand.read("LICENSE-APACHE"), "Apache License\n", "written through the link");
    stand.verify("v2", &second).unwrap();
}

#[test]
fn another_tag_on_the_same_commit_is_recorded() {
    let stand = Stand::new();
    stand.sync("v1", &stand.first).unwrap();
    crate::fixture::git(&stand.upstream, &["checkout", "-q", "--detach", &stand.first.to_string()]);
    stand.tag("v1.0.1");

    let synced = stand.sync("v1.0.1", &stand.first).unwrap();

    assert_eq!(synced, Synced::Already { moved: None });
    assert_eq!(stand.made().release.to_string(), "v1.0.1");
    stand.verify("v1.0.1", &stand.first).unwrap();
}

#[test]
fn a_crate_newly_kept_comes_in_and_one_no_longer_kept_goes() {
    let stand = Stand::new();
    let without_assets = || kept().into_iter().filter(|path| *path != Kept::from("crates/assets")).collect();
    stand.sync_keeping("v1", &stand.first, without_assets()).unwrap();
    assert!(!stand.copy.join("crates/assets").exists());
    write(&stand.copy, &[("crates/base/src/lib.rs", &lines(12, &[(2, "ours")]))]);
    stand.patch("ours").unwrap();

    let synced = stand.sync("v1", &stand.first).unwrap();

    assert_eq!(synced, Synced::Brought { moved: None, dropped: Vec::new() });
    assert_eq!(stand.read("crates/assets/src/lib.rs"), lines(12, &[]));
    assert_eq!(stand.read("crates/base/src/lib.rs"), noticed_rs(&lines(12, &[(2, "ours")])));
    stand.verify("v1", &stand.first).unwrap();
    stand.sync_keeping("v1", &stand.first, without_assets()).unwrap();
    assert!(!stand.copy.join("crates/assets").exists());
}
