//! Builds the tables the file icons are found in, from `file-icons/`, which `tools/file-icons/update.py` writes: every
//! icon there, embedded with its colours filled in from `palette.json` and at the size it's drawn, and the file names
//! and extensions Catppuccin's manifest gives an icon, lowercased and sorted, so the app finds each by a binary search
//! and never reads the manifest itself. On Windows it also puts the app's own icon in the program.

// A module of the build script's own, kept apart from the app's source.
#[path = "build/app_icon.rs"]
mod app_icon;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

use serde_json::Value;

/// How big an icon is drawn, in points. Each is embedded with this size set on it, since GPUI draws an SVG picture at
/// the size it says it is, and a few of Catppuccin's are a point or two taller.
const SIZE: u32 = 16;
/// How an icon names a colour, which `palette.json` says the value of.
const COLOUR: &str = "var(--vscode-ctp-";
/// The elements an icon may be made of, and how big it may be, as `tools/file-icons/update.py` checks before it writes
/// one, so an icon changed by hand since can't be embedded with more than a drawing in it either: no script, text, style
/// sheet, picture from elsewhere, or `<use>`, which can repeat a part of the drawing until it takes minutes to draw.
const DRAWING: [&str; 17] = [
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "rect",
    "line",
    "polyline",
    "polygon",
    "defs",
    "clipPath",
    "mask",
    "linearGradient",
    "radialGradient",
    "stop",
    "title",
    "desc",
];
const MAX_ICON_BYTES: usize = 16 * 1024;
const MAX_ELEMENTS: usize = 64;

fn main() -> Result<(), Box<dyn Error>> {
    app_icon::embed(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("file-icons");
    println!("cargo::rerun-if-changed={}", dir.display());
    let manifest: Value = serde_json::from_slice(&fs::read(dir.join("theme.json"))?)?;
    let palette: BTreeMap<String, String> = serde_json::from_slice(&fs::read(dir.join("palette.json"))?)?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let drawn = out_dir.join("file-icons");
    fs::create_dir_all(&drawn)?;

    let mut out = String::from("// Built by build.rs from file-icons/.\n\n");
    writeln!(
        out,
        "/// How big an icon is drawn, which each is embedded at.\npub(crate) const ICON_SIZE: f32 = {SIZE}.;\n"
    )?;
    let mut icons: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension() != Some("svg".as_ref()) {
            continue;
        }
        let name = path.file_stem().and_then(|stem| stem.to_str()).ok_or("an icon's name isn't UTF-8")?;
        let svg = fs::read_to_string(&path)?;
        if let Some(problem) = more_than_a_drawing(&svg) {
            return Err(format!("{name} {problem}, so it isn't embedded").into());
        }
        let svg = fill(&svg, &palette).map_err(|e| format!("{name}: {e}"))?;
        let svg = with_size(&svg).ok_or_else(|| format!("{name}: its root doesn't say how big it is"))?;
        let copy = drawn.join(format!("{name}.svg"));
        fs::write(&copy, svg)?;
        icons.push((name.to_string(), copy));
    }
    icons.sort();
    writeln!(out, "static ICONS: &[(&str, &[u8])] = &[")?;
    for (name, path) in icons {
        let path = path.to_str().ok_or("the build's folder isn't UTF-8")?;
        writeln!(out, "    ({name:?}, include_bytes!({path:?})),")?;
    }
    writeln!(out, "];\n")?;

    // The app looks a file up by its name alone, without the folders it's in.
    let names = manifest["fileNames"].as_object().ok_or("the manifest has no file names")?;
    if let Some(name) = names.keys().find(|name| name.contains('/')) {
        return Err(format!("the manifest names {name} with its folders, which the app doesn't look up").into());
    }
    write_table(&mut out, "FILE_NAMES", &manifest["fileNames"])?;
    write_table(&mut out, "FILE_EXTENSIONS", &manifest["fileExtensions"])?;
    for (constant, key) in [("FILE", "file"), ("FOLDER", "folder"), ("OPEN_FOLDER", "folderExpanded")] {
        let icon = manifest[key].as_str().ok_or_else(|| format!("the manifest has no {key}"))?;
        writeln!(out, "const {constant}: &str = {icon:?};")?;
    }

    fs::write(out_dir.join("file_icons.rs"), out)?;
    Ok(())
}

/// Why an icon is more than a drawing of one, or `None` if it isn't.
fn more_than_a_drawing(svg: &str) -> Option<String> {
    if svg.len() > MAX_ICON_BYTES {
        return Some(format!("is {} bytes, more than {MAX_ICON_BYTES}", svg.len()));
    }
    // Entities can make a small file expand into a huge one, and no icon needs them, or a comment or an instruction.
    if svg.contains("<!") || svg.contains("<?") {
        return Some("declares something, or has a comment or an instruction".into());
    }
    // A `<` can only start a tag, since text and attributes have to write it as `&lt;`.
    let names = svg.match_indices('<').map(|(at, _)| &svg[at + 1..]).filter(|tag| !tag.starts_with('/'));
    let mut elements = 0;
    for tag in names {
        let name = &tag[..tag.find(|c: char| !c.is_ascii_alphanumeric() && c != ':').unwrap_or(tag.len())];
        if !DRAWING.contains(&name) {
            return Some(format!("has a <{name}>, which isn't part of a drawing"));
        }
        elements += 1;
    }
    if elements > MAX_ELEMENTS {
        return Some(format!("has {elements} elements, more than {MAX_ELEMENTS}"));
    }
    let lower = svg.to_ascii_lowercase();
    // A link or a reference only to a part of the drawing, such as `url(#a)` for a gradient.
    let local = |after: &str| after.trim_start().trim_start_matches(['"', '\'']).trim_start().starts_with('#');
    if lower.match_indices("href=").any(|(at, _)| !local(&lower[at + 5..])) {
        return Some("links to something outside it".into());
    }
    if lower.match_indices("url(").any(|(at, _)| !local(&lower[at + 4..])) {
        return Some("refers to something outside it".into());
    }
    let handler = lower.match_indices("on").any(|(at, _)| {
        let before = lower[..at].chars().next_back().is_some_and(char::is_whitespace);
        let rest = &lower[at + 2..];
        let word = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        before && word > 0 && rest[word..].trim_start().starts_with('=')
    });
    handler.then(|| "has an event handler".into())
}

/// The icon with each colour it names filled in, failing on one the palette doesn't have.
fn fill(svg: &str, palette: &BTreeMap<String, String>) -> Result<String, String> {
    let mut filled = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find(COLOUR) {
        let after = &rest[at + COLOUR.len()..];
        let end = after.find(')').ok_or("a colour's name doesn't end")?;
        let value = palette.get(&after[..end]).ok_or_else(|| format!("the palette has no {}", &after[..end]))?;
        filled.push_str(&rest[..at]);
        filled.push_str(value);
        rest = &after[end + 1..];
    }
    filled.push_str(rest);
    if filled.contains("var(") {
        return Err("it names a colour some other way".into());
    }
    Ok(filled)
}

/// The icon with the size it's drawn at on its root element, in place of the one it says, or `None` if its root
/// doesn't start it, or doesn't say both how wide and how tall it is.
fn with_size(svg: &str) -> Option<String> {
    let svg = svg.trim_start();
    let end = svg.find('>')?;
    let (root, rest) = svg.split_at(end);
    let root = root.strip_prefix("<svg")?;
    let root = set(&set(root, "width")?, "height")?;
    Some(format!("<svg{root}{rest}"))
}

/// The attributes of a root element with `name` set to the drawn size.
fn set(root: &str, name: &str) -> Option<String> {
    let start = root.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = start + root[start..].find('"')?;
    Some(format!("{}{SIZE}{}", &root[..start], &root[end..]))
}

/// One of the manifest's tables, from a name to an icon, with the names lowercased, since editors match them whatever
/// their case. Two names that differ only in case would have to give the same icon, since editors don't say which wins.
fn write_table(out: &mut String, table: &str, entries: &Value) -> Result<(), Box<dyn Error>> {
    let entries = entries.as_object().ok_or_else(|| format!("the manifest's {table} isn't a table"))?;
    let mut lowered: BTreeMap<String, &str> = BTreeMap::new();
    for (name, icon) in entries {
        let icon = icon.as_str().ok_or_else(|| format!("{name}'s icon isn't a name"))?;
        if let Some(other) = lowered.insert(name.to_lowercase(), icon).filter(|other| *other != icon) {
            return Err(format!("{table} gives {name} both {other} and {icon}, whatever its case").into());
        }
    }
    writeln!(out, "static {table}: &[(&str, &str)] = &[")?;
    for (name, icon) in lowered {
        writeln!(out, "    ({name:?}, {icon:?}),")?;
    }
    writeln!(out, "];\n")?;
    Ok(())
}
