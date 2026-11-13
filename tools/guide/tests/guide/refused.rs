//! What the build refuses, each by the kind of error it gives, with nothing written.

use std::fs;

use guide::{Error, NotDrawn};

use super::Fixture;

#[test]
fn a_link_that_points_nowhere_is_refused() {
    let page = Fixture::with_a("# A\n\nSee [gone](gone.md).\n").refused();
    let heading = Fixture::with_a("# A\n\nSee [the part](b/c.md#another-part).\n").refused();
    let file = Fixture::with_a("# A\n\nSee [the license](../../LICENSE).\n").refused();
    let outside = Fixture::with_a("# A\n\nSee [it](../../../elsewhere.md).\n").refused();
    let from_a_root = Fixture::with_a("# A\n\nSee [it](/b/c.md).\n").refused();
    let script = Fixture::with_a("# A\n\nSee [it](javascript://x%0aalert(1)).\n").refused();

    assert!(matches!(page, Error::Missing { ref target, .. } if target == "gone.md"), "{page}");
    assert!(matches!(heading, Error::NoHeading { ref fragment, .. } if fragment == "another-part"), "{heading}");
    assert!(matches!(file, Error::Missing { .. }), "{file}");
    assert!(matches!(outside, Error::Outside { .. }), "{outside}");
    assert!(matches!(from_a_root, Error::Outside { .. }), "{from_a_root}");
    assert!(matches!(script, Error::Unsupported { what: NotDrawn::AddressKind, .. }), "{script}");
}

#[test]
fn contents_that_dont_list_each_page_once_are_refused() {
    let left_out = Fixture::new(&[
        ("docs/guide/README.md", b"# Guide\n\n## First\n\n- [A](a.md)\n"),
        ("docs/guide/a.md", b"# A\n"),
        ("docs/guide/b.md", b"# B\n"),
    ])
    .refused();
    let twice = Fixture::new(&[
        ("docs/guide/README.md", b"# Guide\n\n## First\n\n- [A](a.md)\n  - [A again](a.md)\n"),
        ("docs/guide/a.md", b"# A\n"),
    ])
    .refused();
    let gone = Fixture::of_one_page("# Guide\n\n## First\n\n- [Gone](gone.md)\n").refused();
    let none = Fixture::new(&[("docs/guide/a.md", b"# A\n")]).refused();

    assert!(matches!(left_out, Error::NotInContents(ref page) if page.to_string() == "b.md"), "{left_out}");
    assert!(matches!(twice, Error::ListedTwice(_)), "{twice}");
    assert!(matches!(gone, Error::Missing { ref target, .. } if target == "gone.md"), "{gone}");
    assert!(matches!(none, Error::NoContents), "{none}");
}

#[test]
fn an_item_of_the_contents_that_isnt_another_page_is_refused() {
    let items = [
        "- Coming soon",
        "- [Site](https://example.com/a.md)",
        "- [Notes](../notes.txt)",
        "- [A part](a.md#part)",
        "- [This page](README.md)",
        "- [A file with no name](a/.md)",
    ];
    for item in items {
        let refused = Fixture::new(&[
            ("docs/guide/README.md", format!("# Guide\n\n## First\n\n{item}\n").as_bytes()),
            ("docs/notes.txt", b"Notes."),
            ("docs/guide/a/.md", b"# Hidden\n"),
        ])
        .refused();

        assert!(matches!(refused, Error::NotAPage(_)), "{item}: {refused}");
    }
}

#[test]
fn a_picture_that_cant_be_shown_is_refused() {
    let gone = Fixture::of_one_page("# Guide\n\n![Gone](gone.png)\n").refused();
    let text = Fixture::new(&[
        ("docs/guide/README.md", b"# Guide\n\n![Words](words.png)\n"),
        ("docs/guide/words.png", b"These are words, however the file is named."),
    ])
    .refused();
    let elsewhere = Fixture::of_one_page("# Guide\n\n![On the web](https://example.com/x.png)\n").refused();

    assert!(matches!(gone, Error::Missing { .. }), "{gone}");
    assert!(matches!(text, Error::NotAPng { ref picture, .. } if picture.to_string() == "docs/guide/words.png"));
    assert!(matches!(elsewhere, Error::Unsupported { what: NotDrawn::PictureElsewhere, .. }), "{elsewhere}");
}

#[test]
fn what_a_page_isnt_drawn_with_is_refused() {
    let pages = [
        ("# A\n\n<div>Raw</div>\n", NotDrawn::Html),
        ("# A\n\nSee [C][c].\n\n[c]: b/c.md\n", NotDrawn::NamedApart),
        ("# A\n\nWords.[^1]\n\n[^1]: A note.\n", NotDrawn::Footnote),
        ("# A\n\n- [ ] To do\n", NotDrawn::Task),
        ("# A\n\n# Another title\n", NotDrawn::SecondTitle),
        ("# A\n\n## +\n", NotDrawn::HeadingWithoutWords),
        ("# A\n\n## See [C](b/c.md)\n", NotDrawn::LinkInHeading),
        ("# A\n\n## See *[C](b/c.md)*\n", NotDrawn::LinkInHeading),
    ];
    for (page, expected) in pages {
        let refused = Fixture::with_a(page).refused();

        assert!(matches!(refused, Error::Unsupported { what, .. } if what == expected), "{expected:?}: {refused}");
    }
}

#[test]
fn a_page_the_parser_panics_on_is_refused() {
    let refused = Fixture::with_a("a\n===\n---\nb\n===").refused();

    assert!(matches!(refused, Error::Markdown { .. }), "{refused}");
}

#[test]
fn a_page_that_doesnt_start_with_its_title_is_refused() {
    for page in ["Words before any title.\n\n# A\n", "## A\n", ""] {
        let refused = Fixture::with_a(page).refused();

        assert!(matches!(refused, Error::NoTitle(_)), "{page:?}: {refused}");
    }
}

#[test]
fn a_directory_thats_already_there_isnt_built_into() {
    let fixture = Fixture::with_a("# A\n");
    fs::create_dir_all(&fixture.out).unwrap();

    let refused = fixture.build().unwrap_err();

    assert!(matches!(refused, Error::OutExists(_)), "{refused}");
    assert_eq!(fs::read_dir(&fixture.out).unwrap().count(), 0);
}

/// Only where a file can have such a name: a Mac's filesystem refuses one.
#[cfg(target_os = "linux")]
#[test]
fn a_file_whose_name_isnt_utf8_is_refused() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fixture = Fixture::with_a("# A\n");
    let name = OsStr::from_bytes(b"b\xff.md");
    fs::write(fixture.repo.path().join("docs/guide").join(name), "# B\n").unwrap();

    let refused = fixture.refused();

    assert!(matches!(refused, Error::NotUtf8(_)), "{refused}");
}
