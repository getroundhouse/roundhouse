//! Upstream's workspace manifest, cut down to list only the packages the copy keeps, since Cargo looks for every
//! member.

use toml::{Table, Value};

use crate::error::Error;
use crate::files::CopyPath;

/// The keys whose lists of packages are cut.
const LISTS: [&str; 2] = ["members", "default-members"];

/// `manifest`, which reads as `parsed`, listing only the packages in `crates` but for those in `left_out`. It's cut as
/// text, so the rest of it stays as upstream wrote it, and refused unless it then reads as `parsed` with only those
/// lists changed.
pub(crate) fn members_cut(
    manifest: &str,
    parsed: &Table,
    crates: &[CopyPath],
    left_out: &[CopyPath],
) -> Result<String, Error> {
    let kept = |package: &str| {
        let package = CopyPath::new(package);
        crates.iter().any(|folder| package.within(folder)) && !left_out.iter().any(|folder| package.within(folder))
    };
    let cut_down = cut_lists(manifest, &kept);

    let mut expected = parsed.clone();
    if let Some(workspace) = expected.get_mut("workspace").and_then(Value::as_table_mut) {
        for key in LISTS {
            let packages: Vec<Value> = (workspace.remove(key).as_ref().and_then(Value::as_array).into_iter().flatten())
                .filter(|package| package.as_str().is_some_and(kept))
                .cloned()
                .collect();
            if !packages.is_empty() {
                workspace.insert(key.to_string(), Value::Array(packages));
            }
        }
    }
    if cut_down.parse::<Table>().ok() != Some(expected.clone()) {
        return Err(Error::MembersEntangled);
    }
    let members: Vec<&str> = (expected.get("workspace").and_then(|workspace| workspace.get("members")))
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |members| members.iter().filter_map(Value::as_str).collect());
    let missing: Vec<CopyPath> = crates.iter().filter(|path| !members.contains(&path.as_str())).cloned().collect();
    if !missing.is_empty() {
        return Err(Error::NotMembers(missing));
    }
    Ok(cut_down)
}

/// `manifest` with each list of packages holding only those `kept` says to keep, written one to a line, as upstream
/// writes them, and gone when none is kept.
fn cut_lists(manifest: &str, kept: &impl Fn(&str) -> bool) -> String {
    let mut cut = String::new();
    let mut rest = manifest;
    while !rest.is_empty() {
        if let Some((key, list, length)) = list_at(rest) {
            let packages: Vec<&str> = quoted(list).into_iter().filter(|package| kept(package)).collect();
            if !packages.is_empty() {
                cut.extend([key, " = [\n"]);
                cut.extend(packages.iter().flat_map(|package| ["    \"", package, "\",\n"]));
                cut.push_str("]\n");
            }
            rest = &rest[length..];
        } else {
            let line = rest.find('\n').map_or(rest.len(), |end| end + 1);
            cut += &rest[..line];
            rest = &rest[line..];
        }
    }
    cut
}

/// When `text` starts with a list of packages, its key, what's between its brackets, and how long it is to the end of
/// the line it ends on.
fn list_at(text: &str) -> Option<(&'static str, &str, usize)> {
    let key = LISTS.into_iter().find(|key| text.starts_with(key))?;
    let opened = text[key.len()..].trim_start().strip_prefix('=')?.trim_start().strip_prefix('[')?;
    let (list, after) = opened.split_at(opened.find(']')?);
    let line = after.find('\n')? + 1;
    Some((key, list, text.len() - after.len() + line))
}

/// The strings in quotes in `list`.
fn quoted(list: &str) -> Vec<&str> {
    list.split('"').skip(1).step_by(2).collect()
}
