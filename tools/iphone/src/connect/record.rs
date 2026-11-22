//! The app's record in App Store Connect, which an upload is added to, and which has to exist first.

use anyhow::bail;
use serde_json::Value;

use super::Client;
use crate::xcode::BundleId;

pub(crate) struct AppRecord {
    pub(crate) id: String,
    /// The language the record was made in, which notes for testers are in when a build has none yet.
    pub(crate) primary_locale: String,
}

/// The record of the app with `bundle` as its identifier.
pub(crate) fn find_app(client: &Client<'_>, bundle: &BundleId) -> anyhow::Result<AppRecord> {
    let apps = client.get(&format!("/v1/apps?filter%5BbundleId%5D={}", bundle.as_str()))?;
    let Some(record) = the_app(&apps, bundle.as_str()) else {
        bail!("App Store Connect has no app with that identifier. Its record has to be made there before an upload");
    };
    Ok(record)
}

/// The one of `apps` whose identifier is `bundle`, since App Store Connect's filter matches the start of one too.
fn the_app(apps: &Value, bundle: &str) -> Option<AppRecord> {
    let app = apps["data"].as_array()?.iter().find(|app| app["attributes"]["bundleId"] == bundle)?;
    Some(AppRecord {
        id: app["id"].as_str()?.to_string(),
        primary_locale: app["attributes"]["primaryLocale"].as_str()?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_app_is_the_one_whose_identifier_is_the_whole_of_it() {
        let apps = json!({ "data": [
            { "id": "1", "attributes": { "bundleId": "com.example.app.widget", "primaryLocale": "en-GB" } },
            { "id": "2", "attributes": { "bundleId": "com.example.app", "primaryLocale": "en-US" } },
        ] });

        let app = the_app(&apps, "com.example.app").unwrap();

        assert_eq!((app.id.as_str(), app.primary_locale.as_str()), ("2", "en-US"));
        assert!(the_app(&apps, "com.example").is_none());
    }
}
