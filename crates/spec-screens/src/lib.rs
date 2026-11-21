//! The spec's screens, for the galleries that draw them. Each client's gallery draws its screens from made-up data,
//! and checks each against its approved image, which the spec keeps in `docs/spec/screens/` and shows in its area's
//! file, as `docs/spec/README.md` says. The desktop's images are in that folder, and the phone's in `phone/` inside
//! it, so each gallery knows which images are its own to draw. What each screen looks like now goes in
//! `.context/gallery/` at the repo's root, in the same folders, for looking at. What the screens' chats say is here
//! too, in [`chats`], so a chat reads the same in every client's pictures.

#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

// The fake hub the chats are made for is on a Unix socket.
#[cfg(unix)]
pub mod chats;
#[cfg(unix)]
pub mod proposal_chats;
#[cfg(unix)]
pub mod question_chats;
#[cfg(unix)]
pub mod subagent_chats;

use std::path::{Path, PathBuf};

use image::RgbaImage;

/// Set to accept how every screen looks now, as its approved image.
pub const UPDATE: &str = "ROUNDHOUSE_UPDATE_SCREENS";

/// A picture a gallery took.
pub struct Picture {
    pub image: RgbaImage,
    /// It stopped changing. If not, something on the screen moves, and the picture won't match from run to run.
    pub settled: bool,
}

/// One client's screens, by where its images are kept in `docs/spec/screens/`.
pub struct Screens {
    /// The folder inside `docs/spec/screens/`, or none for that folder itself.
    folder: Option<&'static str>,
}

/// The desktop app's screens, and the design system's sheets at a desk.
pub const DESKTOP: Screens = Screens { folder: None };
/// The iPhone app's screens.
pub const PHONE: Screens = Screens { folder: Some("phone") };

pub enum Verdict {
    Matches,
    Differs,
    Missing,
}

impl Screens {
    /// What's wrong with a picture: its spec doesn't show it, it keeps changing, or it isn't as approved. With
    /// [`UPDATE`] set, it's accepted instead of compared. Either way it's kept to look at.
    pub fn look_at(&self, name: &str, spec: &str, picture: &Picture) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.shown_in(name, spec) {
            problems.push(format!("docs/spec/{spec} doesn't show {name}"));
        }
        self.keep_to_look_at(name, picture);
        // A picture that never stops changing can't look the same from one run to the next, so it can't be compared.
        if !picture.settled {
            problems.push(format!("{name} keeps changing"));
        } else if std::env::var_os(UPDATE).is_some() {
            self.accept(name, &picture.image);
        } else {
            match self.compare(name, &picture.image) {
                Verdict::Matches => {}
                Verdict::Differs => problems.push(format!("{name} no longer matches its image")),
                Verdict::Missing => problems.push(format!("{name} has no image")),
            }
        }
        problems
    }

    /// Puts the picture in `.context/gallery/`, to look at.
    pub fn keep_to_look_at(&self, name: &str, picture: &Picture) {
        let out = self.within(&root().join(".context/gallery"));
        std::fs::create_dir_all(&out).unwrap();
        picture.image.save(out.join(format!("{name}.png"))).unwrap();
    }

    pub fn compare(&self, name: &str, drawn: &RgbaImage) -> Verdict {
        compare_file(&self.path(name), drawn)
    }

    pub fn accept(&self, name: &str, drawn: &RgbaImage) {
        accept_file(&self.path(name), drawn);
    }

    /// Images with no screen to draw them, so nothing checks them any more.
    pub fn without_a_screen(&self, screens: &[&str]) -> Vec<String> {
        let images = std::fs::read_dir(self.dir()).into_iter().flatten().map(|entry| entry.unwrap().path());
        images
            .filter(|path| path.extension().is_some_and(|e| e == "png"))
            .filter_map(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
            .filter(|stem| !screens.contains(&stem.as_str()))
            .collect()
    }

    /// Whether the spec's file for its area shows it, so its image is beside the rules it goes with.
    pub fn shown_in(&self, name: &str, spec: &str) -> bool {
        let text = std::fs::read_to_string(root().join("docs/spec").join(spec)).unwrap_or_default();
        let folder = self.folder.map_or_else(String::new, |folder| format!("{folder}/"));
        text.contains(&format!("](screens/{folder}{name}.png)"))
    }

    /// Where a file kept beside a screen's image is, such as where a sheet draws each name: `name` with its extension.
    pub fn beside(&self, name: &str) -> PathBuf {
        self.dir().join(name)
    }

    fn dir(&self) -> PathBuf {
        self.within(&root().join("docs/spec/screens"))
    }

    fn within(&self, dir: &Path) -> PathBuf {
        self.folder.map_or_else(|| dir.to_path_buf(), |folder| dir.join(folder))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir().join(format!("{name}.png"))
    }
}

/// Exact, because the same screen draws the same pixels every time on one machine, so any difference is a change.
pub fn compare_file(path: &Path, drawn: &RgbaImage) -> Verdict {
    if !path.exists() {
        return Verdict::Missing;
    }
    if image::open(path).unwrap().to_rgba8() == *drawn { Verdict::Matches } else { Verdict::Differs }
}

pub fn accept_file(path: &Path, drawn: &RgbaImage) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    drawn.save(path).unwrap();
}

/// Fails with what's wrong, and how a change that's meant is accepted.
pub fn report(problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each screen looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it, \
         and changing a confirmed screen needs the owner's approval, as docs/spec/README.md says.",
        problems.join("\n")
    );
}

/// The repo's root.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
