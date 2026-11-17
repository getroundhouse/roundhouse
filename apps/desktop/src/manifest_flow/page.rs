//! What the listener answers the browser with: the page that posts the manifest to GitHub's form, which posts itself
//! as it loads, with a button for a browser that runs no scripts, and the short pages that say what happened, each as
//! a whole HTTP response.

use super::listening::ManifestState;

/// What a page says once GitHub has sent the person back.
pub(super) const BACK: &str = "GitHub has made your app. Go back to Roundhouse to install it.";
/// What a page says to a redirect that came after the listener ended.
pub(super) const STOPPED: &str =
    "Roundhouse stopped waiting for GitHub before it sent you back. Go back to Roundhouse to make the app again.";
/// What a page says to a request with no `state`, or another.
pub(super) const NOT_OURS: &str = "This isn't from the app Roundhouse is making. Go back to Roundhouse.";
/// What a page says at any other path.
pub(super) const NOT_HERE: &str = "There's nothing here.";

/// How the listener answers.
#[derive(Clone, Copy)]
pub(super) enum Status {
    Ok,
    BadRequest,
    NotFound,
    Gone,
}

impl Status {
    fn line(self) -> &'static str {
        match self {
            Status::Ok => "200 OK",
            Status::BadRequest => "400 Bad Request",
            Status::NotFound => "404 Not Found",
            Status::Gone => "410 Gone",
        }
    }
}

/// The page that posts `manifest`, with `state`, to GitHub's form at `form`.
pub(super) fn making(manifest: &serde_json::Value, form: &str, state: &ManifestState) -> String {
    let body = format!(
        "<form method=\"post\" action=\"{form}?state={state}\">\
         <input type=\"hidden\" name=\"manifest\" value=\"{manifest}\">\
         <p>Taking you to GitHub to make your app.</p>\
         <button type=\"submit\">Continue to GitHub</button>\
         </form>\
         <script>document.forms[0].submit();</script>",
        form = escaped(form),
        state = escaped(state.as_str()),
        manifest = escaped(&manifest.to_string()),
    );
    whole(&body)
}

/// A page that says `said`, and nothing more.
pub(super) fn said(said: &str) -> String {
    whole(&format!("<p>{}</p>", escaped(said)))
}

/// `page` as the whole of a response with `status`, after which the connection closes. Nothing keeps it, so a browser
/// going back shows none of it again without asking.
pub(super) fn answer(status: Status, page: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n{page}",
        status.line(),
        page.len(),
    )
}

/// `body` in a page of its own, set as plainly as the browser sets text, and in its colours, light or dark.
fn whole(body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>Roundhouse</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:4em auto;max-width:32em;padding:0 1em}}</style>\
         </head><body>{body}</body></html>"
    )
}

/// `text` as HTML takes it in an attribute's quotes or between tags, so nothing in it is read as markup.
pub(super) fn escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}
