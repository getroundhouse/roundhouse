#![cfg(test)]

//! The guide built from made-up repositories. This repository's own guide isn't built here: a change to only its
//! Markdown runs none of these, as AGENTS.md says, so a test of it would fail later, in a pull request that didn't
//! break it. CI's `guide` job builds it instead, for each pull request that changes what it's made of.

use std::fs;
use std::path::PathBuf;

use guide::{Built, Error, Source, build};
use tempfile::TempDir;

mod refused;

/// Contents that list `a.md`, and `b/c.md` under a second section.
const CONTENTS: &str =
    "# Guide\n\nHow to use it.\n\n## First\n\n- [A](a.md): the first page\n\n## Second\n\n- [C](b/c.md)\n";

/// A repository holding `files`, each by its path from the repository's root, and where its guide would be built.
struct Fixture {
    repo: TempDir,
    out: PathBuf,
}

impl Fixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let repo = TempDir::new().unwrap();
        for (path, contents) in files {
            let file = repo.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, contents).unwrap();
        }
        let out = repo.path().join("out/docs");
        Self { repo, out }
    }

    /// A guide of the contents, `a.md` saying `a`, and `b/c.md` with one heading, Part.
    fn with_a(a: &str) -> Self {
        Self::new(&[
            ("docs/guide/README.md", CONTENTS.as_bytes()),
            ("docs/guide/a.md", a.as_bytes()),
            ("docs/guide/b/c.md", b"# C\n\n## Part\n\nWords.\n"),
        ])
    }

    /// A guide that's only the page it opens on, saying `first`.
    fn of_one_page(first: &str) -> Self {
        Self::new(&[("docs/guide/README.md", first.as_bytes())])
    }

    fn build(&self) -> Result<Built, Error> {
        build(&Source::in_repo(self.repo.path()), &self.out)
    }

    /// Why the guide was refused, having checked that a refused guide leaves nothing written.
    fn refused(&self) -> Error {
        let refused = self.build().unwrap_err();
        assert!(!self.out.exists(), "a guide refused with `{refused}` was written all the same");
        refused
    }

    fn page(&self, file: &str) -> String {
        fs::read_to_string(self.out.join(file)).unwrap()
    }
}

/// The start of a PNG of `width` by `height` pixels, which is as much of one as says how big it is.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn a_page_is_published_as_a_directory_and_linked_by_it() {
    let a = "# A\n\nSee [the part](b/c.md#part), [its top](b/c.md#c), [C](./b/c.md \"The 'last' page\"), [the start](../guide/README.md) and [below](#here).\n\n## Here\n";
    let fixture = Fixture::with_a(a);

    let built = fixture.build().unwrap();

    assert_eq!(built, Built { pages: 3, pictures: 0 });
    let a = fixture.page("a/index.html");
    assert!(a.contains("<a class=\"link\" href=\"../b/c/#part\">the part</a>"), "{a}");
    assert!(a.contains("<a class=\"link\" href=\"../b/c/#c\">its top</a>"), "{a}");
    assert!(a.contains("<a class=\"link\" href=\"../b/c/\" title=\"The 'last' page\">C</a>"), "{a}");
    assert!(a.contains("<a class=\"link\" href=\"../\">the start</a>"), "{a}");
    assert!(a.contains("<a class=\"link\" href=\"#here\">below</a>"), "{a}");
    assert!(a.contains("<h1 id=\"a\">A</h1>"), "{a}");
    assert!(a.contains("<h2 id=\"here\"><a href=\"#here\">Here</a></h2>"), "{a}");
    assert!(
        a.contains("href=\"https://github.com/getroundhouse/roundhouse/blob/main/docs/guide/a.md\">Edit this page")
    );
}

#[test]
fn a_page_finds_the_sites_own_files_from_its_depth() {
    let fixture = Fixture::with_a("# A\n");

    fixture.build().unwrap();

    assert!(fixture.page("index.html").contains("href=\"./../style.css\""));
    assert!(fixture.page("a/index.html").contains("href=\"../../style.css\""));
    let c = fixture.page("b/c/index.html");
    assert!(c.contains("href=\"../../../style.css\""), "{c}");
    assert!(c.contains("<a class=\"here\" href=\"../../\">Docs</a>"), "{c}");
}

#[test]
fn the_navigation_follows_the_contents_and_marks_the_page_showing() {
    let fixture = Fixture::with_a("# A\n\nWords.\n");

    fixture.build().unwrap();

    let c = fixture.page("b/c/index.html");
    let navigation = "<nav class=\"guide-nav\" aria-label=\"Guide\">\n<ul>\n<li><a href=\"../../\">Overview</a></li>\n</ul>\n\
        <h2>First</h2>\n<ul>\n<li><a href=\"../../a/\">A</a></li>\n</ul>\n\
        <h2>Second</h2>\n<ul>\n<li><a href=\"../../b/c/\" aria-current=\"page\">C</a></li>\n</ul>\n</nav>";
    // Once beside the page, and once behind the row that a narrow screen shows in its place.
    assert_eq!(c.matches(navigation).count(), 2, "{c}");
}

#[test]
fn a_page_links_to_the_pages_read_before_and_after_it() {
    let fixture = Fixture::with_a("# A\n");

    fixture.build().unwrap();

    let first = fixture.page("index.html");
    assert!(first.contains("<a class=\"after\" href=\"./a/\"><span>Next</span>A</a>"), "{first}");
    assert!(!first.contains("class=\"before\""), "{first}");
    let a = fixture.page("a/index.html");
    assert!(a.contains("<a class=\"before\" href=\"../\"><span>Previous</span>Overview</a>"), "{a}");
    assert!(a.contains("<a class=\"after\" href=\"../b/c/\"><span>Next</span>C</a>"), "{a}");
    let last = fixture.page("b/c/index.html");
    assert!(last.contains("<a class=\"before\" href=\"../../a/\"><span>Previous</span>A</a>"), "{last}");
    assert!(!last.contains("class=\"after\""), "{last}");
}

#[test]
fn a_page_with_several_sections_lists_them_and_one_with_fewer_doesnt() {
    let fixture = Fixture::with_a("# A\n\n## One\n\n### Inside one\n\n## Two\n");

    fixture.build().unwrap();

    let a = fixture.page("a/index.html");
    let list = "<p>On this page</p>\n<ul>\n<li><a href=\"#one\">One</a></li>\n<li><a href=\"#two\">Two</a></li>\n</ul>";
    assert!(a.contains(list), "{a}");
    assert!(a.contains("<aside class=\"guide-headings\""), "{a}");
    assert!(!fixture.page("b/c/index.html").contains("guide-headings"));
}

#[test]
fn a_heading_said_twice_has_the_id_a_link_to_it_was_checked_against() {
    let fixture = Fixture::with_a("# A\n\nSee [the second](#keys-1).\n\n## Keys\n\n## Keys\n");

    fixture.build().unwrap();

    let a = fixture.page("a/index.html");
    assert!(a.contains("<h2 id=\"keys\"><a href=\"#keys\">Keys</a></h2>"), "{a}");
    assert!(a.contains("<h2 id=\"keys-1\"><a href=\"#keys-1\">Keys</a></h2>"), "{a}");
    assert!(a.contains("<a class=\"link\" href=\"#keys-1\">the second</a>"), "{a}");
}

#[test]
fn a_page_under_another_is_listed_under_it() {
    let fixture = Fixture::new(&[
        ("docs/guide/README.md", b"# Guide\n\n## First\n\n- [A](./a.md)\n  - [Part of A](a/part.md)\n"),
        ("docs/guide/a.md", b"# A\n"),
        ("docs/guide/a/part.md", b"# Part\n\nBack to [A](../a.md).\n"),
    ]);

    fixture.build().unwrap();

    let part = fixture.page("a/part/index.html");
    let listed =
        "<li><a href=\"../../a/\">A</a><ul>\n<li><a href=\"../../a/part/\" aria-current=\"page\">Part of A</a></li>";
    assert!(part.contains(listed), "{part}");
    assert!(part.contains("Back to <a class=\"link\" href=\"../../a/\">A</a>."), "{part}");
}

#[test]
fn a_link_out_of_the_guide_goes_where_its_read() {
    let fixture = Fixture::new(&[
        (
            "docs/guide/README.md",
            b"# Guide\n\n[The design](../architecture.md#parts), [the web](https://example.com/a) and [us](mailto:a@example.com).\n",
        ),
        ("docs/architecture.md", b"# Architecture\n"),
    ]);

    fixture.build().unwrap();

    let first = fixture.page("index.html");
    let design = "href=\"https://github.com/getroundhouse/roundhouse/blob/main/docs/architecture.md#parts\">The design";
    assert!(first.contains(design), "{first}");
    assert!(first.contains("<a class=\"link\" href=\"https://example.com/a\">the web</a>"), "{first}");
    assert!(first.contains("<a class=\"link\" href=\"mailto:a@example.com\">us</a>"), "{first}");
}

#[test]
fn a_picture_is_published_beside_the_pages_at_half_its_pixels() {
    let fixture = Fixture::new(&[
        (
            "docs/guide/README.md",
            b"# Guide\n\n![The \"sidebar\"](../spec/screens/sidebar.png 'Its \"left\" edge')\n\n## First\n\n- [A](a.md)\n",
        ),
        ("docs/guide/a.md", b"# A\n\nIt has ![one](../spec/screens/sidebar.png) and ![another](shots/sidebar.png).\n"),
        ("docs/spec/screens/sidebar.png", &png(580, 1720)),
        ("docs/guide/shots/sidebar.png", &png(2, 2)),
    ]);

    let built = fixture.build().unwrap();

    // The first is shown on both pages and published once. The other has its name, in another directory.
    assert_eq!(built.pictures, 2);
    let address = "./images/docs/spec/screens/sidebar.png";
    let picture = format!(
        "<img src=\"{address}\" width=\"290\" height=\"860\" alt=\"The &quot;sidebar&quot;\" title=\"Its &quot;left&quot; edge\" loading=\"lazy\" />"
    );
    let first = fixture.page("index.html");
    assert!(
        first.contains(&format!("<figure class=\"screen\"><a href=\"{address}\">{picture}</a></figure>")),
        "{first}"
    );
    let a = fixture.page("a/index.html");
    assert!(a.contains("<img src=\"../images/docs/spec/screens/sidebar.png\" width=\"290\""), "{a}");
    assert!(a.contains("<img src=\"../images/docs/guide/shots/sidebar.png\" width=\"1\" height=\"1\""), "{a}");
    assert_eq!(fs::read(fixture.out.join("images/docs/spec/screens/sidebar.png")).unwrap(), png(580, 1720));
    assert_eq!(fs::read(fixture.out.join("images/docs/guide/shots/sidebar.png")).unwrap(), png(2, 2));
}

#[test]
fn what_a_page_says_is_escaped_and_never_taken_for_a_place_in_the_template() {
    let fixture = Fixture::with_a(
        "# A & {pager}\n\nUse `<br>` & \"quotes\" in {content}.\n\nTry [a search](https://example.com/?a=1&b=\"2\").\n\n```sh\necho \"<hi>\"\n```\n",
    );

    fixture.build().unwrap();

    let a = fixture.page("a/index.html");
    assert!(a.contains("<title>A &amp; {pager} · Roundhouse</title>"), "{a}");
    let summary = "Use &lt;br&gt; &amp; &quot;quotes&quot; in {content}.";
    assert!(a.contains(&format!("<meta name=\"description\" content=\"{summary}\" />")), "{a}");
    assert!(a.contains("<p>Use <code>&lt;br&gt;</code> &amp; &quot;quotes&quot; in {content}.</p>"), "{a}");
    assert!(a.contains("href=\"https://example.com/?a=1&amp;b=&quot;2&quot;\">a search</a>"), "{a}");
    assert!(a.contains("<pre><code class=\"language-sh\">echo &quot;&lt;hi&gt;&quot;</code></pre>"), "{a}");
}

#[test]
fn a_quote_whose_first_line_is_a_marker_is_a_note_set_apart() {
    let a = "# A\n\n> [!WARNING]\n> Every agent has **full access**.\n\n> [!tip]\n>\n> Plan first.\n\n> [!NOTE] On one line.\n\n> [!NOTE]**Straight** after it.\n\n> [!CAUTION] \\\n> After a break.\n\n> Only a quote.\n";
    let fixture = Fixture::with_a(a);

    fixture.build().unwrap();

    let a = fixture.page("a/index.html");
    let warning = "<aside class=\"callout warning\">\n<p class=\"callout-kind\">Warning</p>\n<p>Every agent has <strong>full access</strong>.</p>\n</aside>";
    let tip = "<aside class=\"callout tip\">\n<p class=\"callout-kind\">Tip</p>\n<p>Plan first.</p>\n</aside>";
    let caution =
        "<aside class=\"callout caution\">\n<p class=\"callout-kind\">Caution</p>\n<p>After a break.</p>\n</aside>";
    assert!(a.contains(warning), "{a}");
    assert!(a.contains(tip), "{a}");
    assert!(a.contains(caution), "{a}");
    // GitHub draws a marker that shares its line as the words they are, so it's a plain quote here too.
    assert!(a.contains("<blockquote>\n<p>[!NOTE] On one line.</p>\n</blockquote>"), "{a}");
    assert!(a.contains("<blockquote>\n<p>[!NOTE]<strong>Straight</strong> after it.</p>\n</blockquote>"), "{a}");
    assert!(a.contains("<blockquote>\n<p>Only a quote.</p>\n</blockquote>"), "{a}");
}

#[test]
fn a_table_and_lists_are_drawn_as_markdown_has_them() {
    let a = "# A\n\n| Key | Does |\n|---|--:|\n| `Cmd-N` | New |\n\n- One\n- Two\n\n3. Three\n4. Four\n\n* Apart\n\n* From this\n\n1. A step\n\n   More about it\n2. The next\n";
    let fixture = Fixture::with_a(a);

    fixture.build().unwrap();

    let a = fixture.page("a/index.html");
    assert!(a.contains("<thead>\n<tr><th>Key</th><th style=\"text-align: right\">Does</th></tr>\n</thead>"), "{a}");
    assert!(a.contains("<tr><td><code>Cmd-N</code></td><td style=\"text-align: right\">New</td></tr>"), "{a}");
    assert!(a.contains("<ul>\n<li>One</li>\n<li>Two</li>\n</ul>"), "{a}");
    assert!(a.contains("<ol start=\"3\">\n<li>Three</li>\n<li>Four</li>\n</ol>"), "{a}");
    // A blank line between items, or within one, gives every item its paragraphs.
    assert!(a.contains("<ul>\n<li><p>Apart</p>\n</li>\n<li><p>From this</p>\n</li>\n</ul>"), "{a}");
    assert!(
        a.contains("<ol>\n<li><p>A step</p>\n<p>More about it</p>\n</li>\n<li><p>The next</p>\n</li>\n</ol>"),
        "{a}"
    );
}
