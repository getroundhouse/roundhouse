//! Which checkout a build of the app is from, and why it's open, so that several open at once, each from its own
//! checkout, can be told apart. Only an unstamped build has one: an install or a package is stamped, as
//! `protocol::build` says, and is the app someone uses rather than one being tried.

use std::path::{Path, PathBuf};

/// Says why the app is open, such as what to try in it: `--note TEXT` or `--note=TEXT`.
pub const NOTE_FLAG: &str = "--note";

/// Where this package was when it was built, if the build isn't stamped. A const, so a stamped build holds no path.
const BUILT_FROM: Option<&str> = match protocol::STAMPED_BUILD {
    None => Some(env!("CARGO_MANIFEST_DIR")),
    Some(_) => None,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    root: PathBuf,
    /// Never empty.
    note: Option<String>,
}

impl Checkout {
    /// The checkout at `root`, the top of its repository. A note that says nothing is none.
    pub fn new(root: PathBuf, note: Option<String>) -> Self {
        let note = note.map(|note| note.trim().to_string()).filter(|note| !note.is_empty());
        Self { root, note }
    }

    /// The checkout this build is from, or `None` for a stamped build.
    pub fn of_this_build(note: Option<String>) -> Option<Self> {
        // The package is `apps/desktop`, two below the top.
        let manifest = Path::new(BUILT_FROM?);
        Some(Self::new(manifest.ancestors().nth(2).unwrap_or(manifest).to_path_buf(), note))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Its directory's name, which for a checkout in a workspace is the workspace's.
    pub fn name(&self) -> String {
        self.root.file_name().map_or_else(|| self.root.display().to_string(), |name| name.to_string_lossy().into())
    }

    /// Why it's open, in the words of whoever opened it.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The window's title, which macOS shows where the window itself can't be seen: in the Window menu, in Mission
    /// Control, and in the menu of the app's icon in the Dock, where every build from a checkout has the same icon.
    pub fn window_title(&self) -> String {
        match &self.note {
            Some(note) => format!("{} — {note}", self.name()),
            None => self.name(),
        }
    }
}

/// Takes the note out of `args`, leaving the rest in order. The last one given wins.
pub fn take_note(args: &mut Vec<String>) -> Option<String> {
    let mut note = None;
    let mut given = std::mem::take(args).into_iter();
    while let Some(arg) = given.next() {
        if arg == NOTE_FLAG {
            note = given.next();
        } else if let Some(text) = arg.strip_prefix(NOTE_FLAG).and_then(|rest| rest.strip_prefix('=')) {
            note = Some(text.to_string());
        } else {
            args.push(arg);
        }
    }
    note
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(given: &[&str]) -> Vec<String> {
        given.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_note_is_taken_out_of_the_arguments_either_way_it_is_given() {
        let mut given = args(&["aspen", "--note", "Check the diff", "diff"]);
        assert_eq!(take_note(&mut given).as_deref(), Some("Check the diff"));
        assert_eq!(given, args(&["aspen", "diff"]));

        let mut given = args(&["--note=Check the diff", "aspen"]);
        assert_eq!(take_note(&mut given).as_deref(), Some("Check the diff"));
        assert_eq!(given, args(&["aspen"]));
    }

    #[test]
    fn the_last_note_wins_and_other_flags_stay() {
        let mut given = args(&["--note", "first", "--notes", "--note=second"]);

        assert_eq!(take_note(&mut given).as_deref(), Some("second"));
        assert_eq!(given, args(&["--notes"]));
    }

    #[test]
    fn a_note_flag_with_nothing_after_it_is_no_note() {
        let mut given = args(&["aspen", "--note"]);
        assert_eq!(take_note(&mut given), None);
        assert_eq!(given, args(&["aspen"]));
    }

    #[test]
    fn a_note_that_says_nothing_is_none() {
        assert_eq!(Checkout::new("/w/aspen".into(), Some("  ".to_string())).note(), None);
        assert_eq!(Checkout::new("/w/aspen".into(), Some(" Check it\n".to_string())).note(), Some("Check it"));
    }

    #[test]
    fn the_title_names_the_checkout_and_says_why_it_is_open() {
        assert_eq!(Checkout::new("/w/aspen".into(), None).window_title(), "aspen");
        let noted = Checkout::new("/w/aspen".into(), Some("Check the diff".to_string()));
        assert_eq!(noted.window_title(), "aspen — Check the diff");
    }

    #[test]
    fn only_an_unstamped_build_is_from_a_checkout_the_one_this_package_is_in() {
        let checkout = Checkout::of_this_build(None);

        match protocol::STAMPED_BUILD {
            None => assert_eq!(checkout.unwrap().root().join("apps/desktop"), Path::new(env!("CARGO_MANIFEST_DIR"))),
            Some(_) => assert_eq!(checkout, None),
        }
    }
}
