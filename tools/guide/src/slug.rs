//! What a link to a heading ends in, made the way GitHub makes it, so a link from one page to a heading in another
//! works where the guide is read on GitHub and on the site.

use std::collections::HashSet;

/// What a link to a heading ends in, after `#`, which is the heading's `id` on its page.
pub(crate) struct Slug(String);

impl Slug {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the heading had nothing a slug keeps, such as one of only punctuation.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A page's slugs, in the order of its headings, which numbers one that's already taken.
#[derive(Default)]
pub(crate) struct Slugs {
    taken: HashSet<String>,
}

impl Slugs {
    /// The slug of the page's next heading, which says `text`: its own if none before it made the same, and with
    /// `-1`, `-2` and so on until it's one no heading before it has.
    pub(crate) fn next(&mut self, text: &str) -> Slug {
        let own = slug(text);
        let mut unique = own.clone();
        let mut again = 0;
        while !self.taken.insert(unique.clone()) {
            again += 1;
            unique = format!("{own}-{again}");
        }
        Slug(unique)
    }
}

/// Lower case, with a hyphen for each space, and without anything but letters, digits, hyphens and underscores.
fn slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_is_what_github_makes_of_a_heading() {
        assert_eq!(slug("Steer, or queue?"), "steer-or-queue");
        assert_eq!(slug("The roundhouse command"), "the-roundhouse-command");
        assert_eq!(slug("The new-workspace dialog"), "the-new-workspace-dialog");
        assert_eq!(slug("CONDUCTOR_PORT and friends"), "conductor_port-and-friends");
        assert_eq!(slug("Mac — Apple silicon"), "mac--apple-silicon");
        assert_eq!(slug("+"), "");
    }

    #[test]
    fn a_heading_said_again_is_numbered_past_any_that_took_its_number() {
        let mut slugs = Slugs::default();

        let made: Vec<Slug> = ["Keys", "Keys", "Keys 1", "Other", "Keys"].iter().map(|text| slugs.next(text)).collect();

        let made: Vec<&str> = made.iter().map(Slug::as_str).collect();
        assert_eq!(made, ["keys", "keys-1", "keys-1-1", "other", "keys-2"]);
    }
}
