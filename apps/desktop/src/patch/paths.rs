//! The paths in a patch's headers. Git quotes a path that has unusual characters in it, as C quotes a string, and
//! puts a prefix before each side's path, which the user's settings can change or turn off.

/// The prefixes git can put before the old and new paths, with its own first. The others are `diff.mnemonicPrefix`'s,
/// and the last is `diff.noprefix`'s.
const PREFIXES: [(&str, &str); 7] =
    [("a/", "b/"), ("c/", "w/"), ("i/", "w/"), ("c/", "i/"), ("o/", "w/"), ("1/", "2/"), ("", "")];

/// The prefixes a patch's paths have.
pub(super) type Prefixes = (&'static str, &'static str);

/// The two paths of a `diff --git` line, after the `diff --git `, without their prefixes, and the prefixes they had.
pub(super) fn git_paths(header: &str) -> Option<(String, String, Prefixes)> {
    let (old, new) = if header.starts_with('"') {
        let end = quoted_end(header)?;
        (decode(&header[..end]), decode(header[end..].trim_start()))
    } else if header.ends_with('"') {
        // Only the new path is quoted, so the old one is everything before the quote that ends at the end.
        let start = header
            .match_indices(" \"")
            .map(|(ix, _)| ix + 1)
            .find(|&ix| quoted_end(&header[ix..]) == Some(header.len() - ix))?;
        (header[..start - 1].to_string(), decode(&header[start..]))
    } else {
        return unquoted(header);
    };
    let prefixes = PREFIXES.into_iter().find(|(o, n)| old.starts_with(o) && new.starts_with(n)).unwrap_or(("", ""));
    Some((strip(&old, prefixes.0), strip(&new, prefixes.1), prefixes))
}

/// Two paths that aren't quoted, which can have spaces in them. Without a rename both are the same path, which says
/// which space is between them. With one, the patch has the exact paths in its `rename from` and `rename to` lines,
/// so a guess at the new prefix's last place does.
fn unquoted(header: &str) -> Option<(String, String, Prefixes)> {
    let same = PREFIXES.into_iter().find_map(|prefixes| {
        header.match_indices(' ').find_map(|(ix, _)| {
            let (old, new) = (&header[..ix], &header[ix + 1..]);
            let path = old.strip_prefix(prefixes.0)?;
            (!path.is_empty() && new.strip_prefix(prefixes.1) == Some(path)).then(|| (path.to_string(), prefixes))
        })
    });
    if let Some((path, prefixes)) = same {
        return Some((path.clone(), path, prefixes));
    }
    let split = PREFIXES.into_iter().filter(|(old, _)| !old.is_empty()).find_map(|prefixes| {
        let ix = header.rfind(&format!(" {}", prefixes.1))?;
        header.starts_with(prefixes.0).then_some((ix, prefixes))
    });
    let (ix, prefixes) = split.or_else(|| header.find(' ').map(|ix| (ix, ("", ""))))?;
    Some((strip(&header[..ix], prefixes.0), strip(&header[ix + 1..], prefixes.1), prefixes))
}

/// A `---` or `+++` line's path, after the marker, without `prefix`. `None` for `/dev/null`, the side a file that's
/// added or removed hasn't got. Git ends a path that has a space in it with a tab, and other tools put a date there.
pub(super) fn header_path(value: &str, prefix: &str) -> Option<String> {
    let value = value.split('\t').next().unwrap_or(value);
    (value != "/dev/null").then(|| strip(&decode(value), prefix))
}

/// A path from an extended header, such as `rename from`, which has no prefix.
pub(super) fn decode(value: &str) -> String {
    if !value.starts_with('"') || quoted_end(value) != Some(value.len()) {
        return value.to_string();
    }
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value[1..value.len() - 1].bytes().peekable();
    while let Some(byte) = rest.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let Some(escaped) = rest.next() else { break };
        bytes.push(match escaped {
            b'0'..=b'7' => {
                // Up to three octal digits, one byte of the path, so a name in UTF-8 is a few of these in a row.
                let mut value = u32::from(escaped - b'0');
                for _ in 0..2 {
                    let Some(digit) = rest.next_if(|digit| (b'0'..=b'7').contains(digit)) else { break };
                    value = value * 8 + u32::from(digit - b'0');
                }
                u8::try_from(value).unwrap_or(b'?')
            }
            b'a' => 7,
            b'b' => 8,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 11,
            b'f' => 12,
            b'r' => b'\r',
            other => other,
        });
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Where the quoted string that `value` starts with ends, just after its closing quote.
fn quoted_end(value: &str) -> Option<usize> {
    let mut escaped = false;
    for (ix, character) in value.char_indices().skip(1) {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(ix + 1),
            _ => {}
        }
    }
    None
}

fn strip(path: &str, prefix: &str) -> String {
    path.strip_prefix(prefix).filter(|_| !prefix.is_empty()).unwrap_or(path).to_string()
}
