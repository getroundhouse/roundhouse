//! What an app says when it and a paired hub share no version of the protocol: which of the two needs updating.

use client_core::OlderSide;

/// What an app says of a protocol break with a paired hub, as a heading and what's under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedsUpdating {
    pub heading: String,
    pub says: String,
}

/// Which of the hub called `hub` and this app needs updating. `device` is what the app runs on, after "this", such as
/// "Mac", and `app` is the app's release, for an app that has one to say. It never says how the hub is updated, which a
/// paired device can't tell.
pub fn needs_updating(hub: &str, device: &str, app: Option<&str>, older: &OlderSide) -> NeedsUpdating {
    let app = app.map_or_else(|| format!("this {device}"), |app| format!("this {device}’s {app}"));
    match older {
        OlderSide::Hub { version } => {
            let runs = version.as_ref().map_or_else(
                || format!("It runs a Roundhouse too old for {app} to connect to."),
                |version| format!("It runs Roundhouse {version}, which is too old for {app} to connect to."),
            );
            NeedsUpdating {
                heading: format!("{hub} needs updating"),
                says: format!("{runs} Update Roundhouse on {hub}, and this {device} reconnects by itself."),
            }
        }
        OlderSide::Client { hub_version } => NeedsUpdating {
            heading: format!("Roundhouse on this {device} needs updating"),
            says: format!("{hub} runs Roundhouse {hub_version}, which {app} is too old to connect to."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_older_side_is_the_one_to_update() {
        let hub = |version: Option<&str>| OlderSide::Hub { version: version.map(str::to_string) };
        let client = OlderSide::Client { hub_version: "0.1.9".to_string() };
        let cases = [
            (
                needs_updating("Studio", "Mac", Some("0.1.9"), &hub(Some("0.1.5"))),
                "Studio needs updating",
                "It runs Roundhouse 0.1.5, which is too old for this Mac’s 0.1.9 to connect to. Update Roundhouse on \
                 Studio, and this Mac reconnects by itself.",
            ),
            (
                needs_updating("Studio", "Mac", Some("0.1.9"), &hub(None)),
                "Studio needs updating",
                "It runs a Roundhouse too old for this Mac’s 0.1.9 to connect to. Update Roundhouse on Studio, and \
                 this Mac reconnects by itself.",
            ),
            (
                needs_updating("Studio", "Mac", Some("0.1.5"), &client),
                "Roundhouse on this Mac needs updating",
                "Studio runs Roundhouse 0.1.9, which this Mac’s 0.1.5 is too old to connect to.",
            ),
            (
                needs_updating("Studio", "iPhone", None, &client),
                "Roundhouse on this iPhone needs updating",
                "Studio runs Roundhouse 0.1.9, which this iPhone is too old to connect to.",
            ),
        ];

        for (said, heading, says) in cases {
            assert_eq!((said.heading.as_str(), said.says.as_str()), (heading, says));
        }
    }
}
