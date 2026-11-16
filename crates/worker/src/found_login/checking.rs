use std::path::Path;
use std::time::Duration;

use process::Environment;
use protocol::model::GhLoginWorks;
use serde::Deserialize;

use crate::gh::run_gh_within;

/// What `gh` is asked: GitHub's answer for whose its token is, with the status line and the headers before the body,
/// so the answer is GitHub's own status rather than what `gh` makes of it.
const ASKED: [&str; 3] = ["api", "--include", "user"];

/// Whether GitHub takes the token of the login `gh` names `username`, asked within `within`, as
/// [`GhLoginWorks`] says.
pub(super) async fn works(within: Duration, given: &Environment, dir: &Path, username: &str) -> GhLoginWorks {
    match run_gh_within(within, given, dir, &ASKED).await {
        Ok(printed) => answered(&printed.stdout, username),
        Err(_) => GhLoginWorks::CouldntTell,
    }
}

/// What GitHub's answer says, by its status, and for a success, by the user it names, which must be `username`, as
/// GitHub's usernames are, whatever their case. With no status, `gh` got no answer, as when GitHub can't be reached.
fn answered(printed: &[u8], username: &str) -> GhLoginWorks {
    let Some((status, body)) = std::str::from_utf8(printed).ok().and_then(answer) else {
        return GhLoginWorks::CouldntTell;
    };
    match status {
        200..=299 if named(body).is_some_and(|login| login.eq_ignore_ascii_case(username)) => GhLoginWorks::Yes,
        401 => GhLoginWorks::Refused,
        _ => GhLoginWorks::CouldntTell,
    }
}

/// The status and the body of what `gh api --include` printed: the status line first, as `HTTP/2.0 200 OK`, then the
/// headers, then a blank line and the body. `gh` ends the status line with `\n` and the rest with `\r\n`.
fn answer(printed: &str) -> Option<(u16, &str)> {
    let mut lines = printed.split_inclusive('\n');
    let status_line = lines.next()?;
    let mut words = status_line.split_whitespace();
    words.next().filter(|protocol| protocol.starts_with("HTTP/"))?;
    let status = words.next()?.parse().ok()?;
    let mut read = status_line.len();
    for line in lines {
        read += line.len();
        if line.trim_end_matches(['\r', '\n']).is_empty() {
            return Some((status, &printed[read..]));
        }
    }
    None
}

/// Who GitHub said the token's user is, as far as it's read.
#[derive(Deserialize)]
struct User {
    login: String,
}

fn named(body: &str) -> Option<String> {
    serde_json::from_str::<User>(body).ok().map(|user| user.login)
}
