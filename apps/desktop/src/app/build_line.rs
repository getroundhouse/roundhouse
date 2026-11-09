//! What the bottom of the sidebar says about which build is running, and about updating it.

/// One build is all most people need to know about, so the hub only comes up when it isn't the app's build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildLine {
    /// The app and its hub are the same build, or there's no hub to compare it with.
    Current { app: String },
    /// A newer build is downloaded, which installs when the app restarts or quits.
    UpdateReady { app: String, next: String },
    /// The app has updated, and its hub is still the older build, with work on it, such as `agents` running or
    /// waiting for an answer. The app restarts the hub once that work is done, or sooner if asked.
    Finishing { app: String, hub: String, agents: usize },
    /// The hub is another build that the app leaves alone: one someone started themselves, or a newer one.
    OtherHub { app: String, hub: String },
}

impl BuildLine {
    /// `hub` is `None` while there's no connection. `may_restart` says whether the app may restart the hub it's
    /// connected to, `agents` how many are running on it, and `ready` is the version of an update that's downloaded.
    pub(super) fn new(app: &str, hub: Option<&str>, may_restart: bool, agents: usize, ready: Option<&str>) -> Self {
        let app = app.to_string();
        if let Some(next) = ready {
            return Self::UpdateReady { app, next: next.to_string() };
        }
        match hub {
            Some(hub) if hub != app && may_restart => Self::Finishing { app, hub: hub.to_string(), agents },
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
        assert_eq!(BuildLine::new("abc", Some("abc"), true, 0, None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn with_no_connection_there_is_nothing_to_compare() {
        assert_eq!(BuildLine::new("abc", None, true, 0, None), BuildLine::Current { app: "abc".to_string() });
    }

    #[test]
    fn an_older_hub_the_app_may_restart_is_finishing_the_update() {
        let line = BuildLine::new("abc", Some("def"), true, 2, None);
        assert_eq!(line, BuildLine::Finishing { app: "abc".to_string(), hub: "def".to_string(), agents: 2 });
    }

    #[test]
    fn a_hub_the_app_leaves_alone_is_named() {
        let line = BuildLine::new("abc", Some("def"), false, 0, None);
        assert_eq!(line, BuildLine::OtherHub { app: "abc".to_string(), hub: "def".to_string() });
    }

    #[test]
    fn a_downloaded_update_comes_first() {
        let line = BuildLine::new("abc", Some("def"), true, 0, Some("ghi"));
        assert_eq!(line, BuildLine::UpdateReady { app: "abc".to_string(), next: "ghi".to_string() });
    }
}
