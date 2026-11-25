use http::HeaderMap;
use url::form_urlencoded;

/// One request the stand-in was sent.
#[derive(Clone, Debug)]
pub struct Asked {
    /// Its method and path, such as `POST /token`, with no query.
    pub line: String,
    pub headers: HeaderMap,
    pub body: String,
}

impl Asked {
    /// What its body, a form, says `name` is.
    pub fn form(&self, name: &str) -> Option<String> {
        form_urlencoded::parse(self.body.as_bytes()).find(|(said, _)| said == name).map(|(_, value)| value.into_owned())
    }
}
