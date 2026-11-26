//! The pages the listener answers the browser with, each as a whole HTTP response, set as plainly as the browser sets
//! text, with what's in them made safe in HTML.

/// How the listener answers.
#[derive(Clone, Copy)]
pub(crate) enum Status {
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

/// A page that says `said`, and nothing more.
pub(crate) fn said(said: &str) -> String {
    whole(&format!("<p>{}</p>", escaped(said)))
}

/// The page where the listener has nothing.
pub(super) fn nothing_here() -> String {
    said("There's nothing here.")
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
pub(crate) fn whole(body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>Roundhouse</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:4em auto;max-width:32em;padding:0 1em}}</style>\
         </head><body>{body}</body></html>"
    )
}

/// `text` as HTML takes it in an attribute's quotes or between tags, so nothing in it is read as markup.
pub(crate) fn escaped(text: &str) -> String {
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
