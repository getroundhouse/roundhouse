//! What the listener answers the browser with: the page that posts the manifest to GitHub's form, which posts itself
//! as it loads, with a button for a browser that runs no scripts, and the short pages that say what happened.

use super::listening::ManifestState;
use crate::loopback::page::{escaped, whole};

/// What a page says once GitHub has sent the person back.
pub(super) const BACK: &str = "GitHub has made your app. Go back to Roundhouse to install it.";
/// What a page says to a redirect that came after the listener ended.
pub(super) const STOPPED: &str =
    "Roundhouse stopped waiting for GitHub before it sent you back. Go back to Roundhouse to make the app again.";
/// What a page says to a request with no `state`, or another.
pub(super) const NOT_OURS: &str = "This isn't from the app Roundhouse is making. Go back to Roundhouse.";

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
