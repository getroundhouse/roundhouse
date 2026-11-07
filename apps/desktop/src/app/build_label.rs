//! What the sidebar says about which build is running.

/// Names the hub as well when it's a different build, because then what's on screen and what's doing the
/// work aren't the same code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildLabel {
    pub text: String,
    pub differs: bool,
}

impl BuildLabel {
    /// `hub` is `None` while there's no connection, when only the app's own build is known.
    pub(super) fn new(app: &str, hub: Option<&str>) -> Self {
        match hub {
            Some(hub) if hub != app => Self { text: format!("app {app}, hub {hub}"), differs: true },
            Some(_) | None => Self { text: app.to_string(), differs: false },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_build_is_only_named_once() {
        assert_eq!(BuildLabel::new("abc", Some("abc")), BuildLabel { text: "abc".to_string(), differs: false });
    }

    #[test]
    fn a_hub_on_another_build_is_called_out() {
        let label = BuildLabel::new("abc", Some("def"));

        assert!(label.differs);
        assert_eq!(label.text, "app abc, hub def");
    }

    #[test]
    fn with_no_connection_there_is_nothing_to_compare() {
        assert_eq!(BuildLabel::new("abc", None), BuildLabel { text: "abc".to_string(), differs: false });
    }
}
