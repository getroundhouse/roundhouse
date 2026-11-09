//! What the bottom of the sidebar says about which build is running, and about updating it.

/// One build is all most people need to know about, so the hub only comes up when it isn't the app's build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildLine {
    /// The app and its hub are the same build, or there's no hub to compare it with.
    Current { app: String },
    /// A newer build is downloaded, which installs when the app restarts or quits.
    UpdateReady { app: String, next: String },
    /// The hub is another build.
    OtherHub { app: String, hub: String },
}

impl BuildLine {
    /// `hub` is `None` while there's no connection, and `ready` is the version of an update that's downloaded.
    pub(super) fn new(app: &str, hub: Option<&str>, ready: Option<&str>) -> Self {
        let app = app.to_string();
        if let Some(next) = ready {
            return Self::UpdateReady { app, next: next.to_string() };
        }
        match hub {
            Some(hub) if hub != app => Self::OtherHub { app, hub: hub.to_string() },
            Some(_) | None => Self::Current { app },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_build_is_only_named_once() {
        assert_eq!(BuildLine::new("abc", Some("abc"), None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn with_no_connection_there_is_nothing_to_compare() {
        assert_eq!(BuildLine::new("abc", None, None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn a_hub_on_another_build_is_named() {
        let line = BuildLine::new("abc", Some("def"), None);
        assert_eq!(line, BuildLine::OtherHub { app: "abc".to_string(), hub: "def".to_string() });
    }

    #[test]
    fn a_downloaded_update_comes_first() {
        let line = BuildLine::new("abc", Some("def"), Some("ghi"));
        assert_eq!(line, BuildLine::UpdateReady { app: "abc".to_string(), next: "ghi".to_string() });
    }
}
