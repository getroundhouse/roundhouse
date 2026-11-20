//! That every icon the code names is embedded where its window loads it, since one that isn't draws nothing and says
//! nothing. The kit embeds some by default, `look::Assets` the rest of those that the crates every client shares draw,
//! and each app its own, as `crates/look/src/assets.rs` says.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::rust_standards::{add_rust_files, covered_crates};
use super::workspace::workspace;

/// Where the kit lists the icons it embeds by default, from the repo root.
const KIT_DEFAULTS: &str = "third_party/gpui-kit/vendor/crates/assets/default-icons.txt";
/// Where look lists the icons of what every client shares.
const LOOK: &str = "crates/look/src/assets.rs";
/// Each app's own list, by the folder its code is in.
const APPS: &[(&str, &str)] = &[("apps/desktop/", "apps/desktop/src/assets.rs")];

/// An icon's name, as code and a file name have it, such as `Columns2` and `columns-2`, made alike to compare.
fn alike(name: &str) -> String {
    name.chars().filter(|c| *c != '-').flat_map(char::to_lowercase).collect()
}

/// The icons the list in the file at `path` embeds, as `icon_assets!` has them.
fn listed(root: &Path, path: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(path)).unwrap();
    let start = text.find("icon_assets!(").unwrap_or_else(|| panic!("{path} has no icon_assets!"));
    let list = &text[start..];
    let list = &list[list.find('[').unwrap() + 1..list.find(']').unwrap()];
    list.split(',').map(str::trim).filter(|name| !name.is_empty()).map(alike).collect()
}

/// The icons the kit embeds by default.
fn kit_defaults(root: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(KIT_DEFAULTS)).unwrap();
    text.lines().filter_map(|line| line.strip_prefix("icons/")?.strip_suffix(".svg")).map(alike).collect()
}

/// Each icon a file in the source of the crates the checks cover names in its code, by the files that name it, from
/// the repo root. An icon is named as `IconName::Name`, or by another name it's imported as, such as `Lucide::Name`.
fn named(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut files = Vec::new();
    for dir in covered_crates(&workspace()) {
        let source = dir.join("src");
        if source.is_dir() {
            add_rust_files(&source, &mut files);
        }
    }
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut types = vec!["IconName".to_string()];
        types.extend(text.match_indices("IconName as ").filter_map(|(at, found)| {
            let alias: String =
                text[at + found.len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            (!alias.is_empty()).then_some(alias)
        }));
        let path = file.strip_prefix(root).unwrap().display().to_string();
        for line in text.lines().filter(|line| !line.trim_start().starts_with("//")) {
            for name in &types {
                let mention = format!("{name}::");
                for (at, _) in line.match_indices(&mention) {
                    // In a string, it's words rather than code.
                    if line[..at].matches('"').count() % 2 == 1 {
                        continue;
                    }
                    let before = line[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let icon: String = line[at + mention.len()..].chars().take_while(|c| c.is_alphanumeric()).collect();
                    if icon.starts_with(|c: char| c.is_ascii_uppercase()) {
                        named.entry(alike(&icon)).or_default().insert(path.clone());
                    }
                }
            }
        }
    }
    named
}

#[test]
fn every_icon_the_code_names_is_embedded_where_its_window_loads_it() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let (kit, look) = (kit_defaults(root), listed(root, LOOK));
    let named = named(root);
    // Every app names a few, so finding none means the search went wrong.
    assert!(named.len() > 20, "found only {} icons named, so this isn't looking in the right place", named.len());
    let mut missing = Vec::new();
    for (icon, files) in &named {
        for file in files {
            let app = APPS.iter().find(|(folder, _)| file.starts_with(folder));
            let own = app.map_or_default(|(_, list)| listed(root, list));
            if !kit.contains(icon) && !look.contains(icon) && !own.contains(icon) {
                let place = app.map_or(LOOK, |(_, list)| list);
                missing.push(format!("{icon}, named in {file}, belongs in {place}"));
            }
        }
    }
    assert!(missing.is_empty(), "icons nothing embeds, which draw as nothing:\n{}", missing.join("\n"));
}

#[test]
fn each_icon_is_embedded_once_and_only_where_its_drawn() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let (kit, look) = (kit_defaults(root), listed(root, LOOK));
    let named = named(root);
    let mut wrong = Vec::new();
    for icon in &look {
        if kit.contains(icon) {
            wrong.push(format!("{icon} is in {LOOK}, and the kit embeds it already"));
        }
        let shared = named.get(icon).is_some_and(|files| files.iter().any(|file| !file.starts_with("apps/")));
        if !shared {
            wrong.push(format!("{icon} is in {LOOK}, and no crate every client shares names it"));
        }
    }
    for (folder, list) in APPS {
        for icon in listed(root, list) {
            if kit.contains(&icon) || look.contains(&icon) {
                wrong.push(format!("{icon} is in {list}, and the kit or {LOOK} embeds it already"));
            }
            if !named.get(&icon).is_some_and(|files| files.iter().any(|file| file.starts_with(folder))) {
                wrong.push(format!("{icon} is in {list}, and nothing in {folder} names it"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
