use super::*;

/// Every table a name is looked up in.
const TABLES: [&[(&str, &str)]; 2] = [FILE_NAMES, FILE_EXTENSIONS];

/// Every icon a name can get.
fn named() -> Vec<&'static str> {
    let mut named: Vec<&str> = TABLES
        .iter()
        .flat_map(|table| table.iter().map(|(_, icon)| *icon))
        .chain([FILE, FOLDER, OPEN_FOLDER])
        .collect();
    named.sort_unstable();
    named.dedup();
    named
}

#[test]
fn every_icon_a_name_can_get_is_embedded() {
    // A name the asset source can't find draws as nothing at all, rather than failing.
    let missing: Vec<&str> = named().into_iter().filter(|icon| load(&asset(icon)).is_none()).collect();
    assert!(missing.is_empty(), "named but not embedded: {missing:?}");
}

#[test]
fn every_icon_embedded_is_one_a_name_can_get() {
    // `tools/file-icons/update.py` keeps only these, so the app carries no icon it can't show.
    let named = named();
    let unnamed: Vec<&str> =
        ICONS.iter().map(|(name, _)| *name).filter(|name| named.binary_search(name).is_err()).collect();
    assert!(unnamed.is_empty(), "embedded but never shown: {unnamed:?}");
}

#[test]
fn every_table_is_in_order_of_name() {
    // They're searched by halves, which finds nothing in a table out of order.
    assert!(ICONS.is_sorted_by_key(|(name, _)| *name));
    for table in TABLES {
        assert!(table.is_sorted_by_key(|(name, _)| *name));
    }
}

#[test]
fn every_icon_is_in_its_colours_and_of_the_size_it_s_drawn_at() {
    // GPUI draws an SVG picture at the size it says it is, so one that says it's taller would be drawn taller.
    let sized = format!(" width=\"{ICON_SIZE}\" height=\"{ICON_SIZE}\"");
    let wrong_size: Vec<&str> = ICONS
        .iter()
        .filter(|(_, svg)| {
            let svg = String::from_utf8_lossy(svg);
            let root = svg.split('>').next().unwrap_or_default();
            !(root.starts_with("<svg") && root.contains(&sized))
        })
        .map(|(name, _)| *name)
        .collect();
    assert!(wrong_size.is_empty(), "not drawn at {ICON_SIZE}: {wrong_size:?}");
    // A colour left named, as `var(--vscode-ctp-blue)`, would draw as nothing.
    let unfilled: Vec<&str> = ICONS.iter().filter(|(_, svg)| contains(svg, b"var(")).map(|(name, _)| *name).collect();
    assert!(unfilled.is_empty(), "colours not filled in: {unfilled:?}");
}

#[test]
fn the_plain_file_and_the_folders_are_drawn_in_one_colour() {
    // They're drawn as icons in the text colour they're given, which keeps only their shape.
    for name in [FILE, FOLDER, OPEN_FOLDER] {
        let svg = String::from_utf8_lossy(load(&asset(name)).unwrap_or_default()).to_string();
        let mut colours: Vec<&str> = svg.match_indices('#').map(|(at, _)| &svg[at..(at + 7).min(svg.len())]).collect();
        colours.sort_unstable();
        colours.dedup();
        assert_eq!(colours.len(), 1, "{name} has colours {colours:?}");
    }
}

#[test]
fn a_file_gets_the_icon_the_manifest_gives_its_name_and_then_its_longest_extension() {
    for (path, icon) in [
        ("crates/hub/src/hub.rs", "rust"),
        ("Cargo.lock", "cargo-lock"),
        ("Cargo.toml", "cargo"),
        ("rust-toolchain.toml", "rust-config"),
        (".env", "env"),
        ("apps/web/.env.local", "env"),
        ("README.md", "readme"),
        ("AGENTS.md", "markdown"),
        (".gitignore", "git"),
        (".dockerignore", "docker-ignore"),
        ("web/src/app.spec.ts", "typescript-test"),
        ("web/src/types.d.ts", "typescript-def"),
        ("Makefile", "makefile"),
        ("LICENSE", "license"),
        ("shots/Photo.PNG", "image"),
        ("Dockerfile", "docker"),
        ("Dockerfile.dev", "docker"),
        ("services/Dockerfile.api", "docker"),
        ("logs/app.log", "log"),
        ("logs/app.log.1", "log"),
        ("logs/app.log.12", "_file"),
        ("notes", "_file"),
        ("data.unknown", "_file"),
    ] {
        assert_eq!(file_kind(path), icon, "{path}");
    }
}

fn contains(svg: &[u8], part: &[u8]) -> bool {
    svg.windows(part.len()).any(|window| window == part)
}
