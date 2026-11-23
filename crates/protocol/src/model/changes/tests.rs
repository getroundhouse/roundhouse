//! The order a summary and its pages list files in, and what each leaves out.

use super::*;

fn file(path: &str, status: ChangeStatus, lines: Option<LineCounts>) -> ChangedFile {
    ChangedFile { path: path.into(), from: None, status, lines, version: None }
}

#[test]
fn a_summary_s_lines_are_the_sums_of_its_files_and_a_file_with_none_counted_adds_none() {
    let files = vec![
        file("a.rs", ChangeStatus::Modified, Some(LineCounts { added: 3, removed: 1 })),
        file("b.png", ChangeStatus::Modified, None),
        file("c.rs", ChangeStatus::Modified, Some(LineCounts { added: 0, removed: 4 })),
    ];

    let summary = ChangeSummary::new("c1".into(), files);

    assert_eq!(summary.lines, LineCounts { added: 3, removed: 5 });
    assert_eq!(summary.left_out, 0);
}

/// Past the bound, a summary lists tracked files before untracked ones, each in order of their paths, and leaves
/// out the rest, whose lines its totals still count.
#[test]
fn a_summary_lists_tracked_files_first_up_to_its_bound_and_sums_those_it_leaves_out() {
    let one = Some(LineCounts { added: 1, removed: 0 });
    let mut files =
        Vec::from_fn(MOST_FILES_LISTED, |n| file(&format!("node_modules/{n:05}.js"), ChangeStatus::Untracked, one));
    files.extend([
        file("src/b.rs", ChangeStatus::Modified, Some(LineCounts { added: 2, removed: 3 })),
        file("src/a.rs", ChangeStatus::Deleted, None),
        file("a.txt", ChangeStatus::Untracked, one),
    ]);

    let summary = ChangeSummary::new("c1".into(), files);

    let paths = summary.files.iter().map(|file| file.path.as_str());
    assert_eq!(summary.files.len(), MOST_FILES_LISTED);
    assert_eq!(paths.clone().take(4).collect::<Vec<_>>(), ["src/a.rs", "src/b.rs", "a.txt", "node_modules/00000.js"]);
    assert_eq!(summary.files.last().map(|file| file.path.as_str()), Some("node_modules/04996.js"));
    assert_eq!(summary.left_out, 3);
    let untracked = u64::try_from(MOST_FILES_LISTED).unwrap() + 1;
    assert_eq!(summary.lines, LineCounts { added: untracked + 2, removed: 3 });
}

/// A page starts after the file it was asked to follow, in the summary's order, whether or not that file is still
/// changed, and says how many come after it, 0 on the last.
#[test]
fn a_page_lists_the_files_after_the_one_it_follows_and_how_many_are_left_after_it() {
    let files = || {
        vec![
            file("b.rs", ChangeStatus::Untracked, None),
            file("a.rs", ChangeStatus::Untracked, None),
            file("c.rs", ChangeStatus::Modified, None),
            file("a.rs", ChangeStatus::Deleted, None),
            file("d.rs", ChangeStatus::Untracked, None),
        ]
    };
    let paths = |page: &ChangedFilesPage| page.files.iter().map(ChangedFile::at).collect::<Vec<_>>();
    let at = |untracked, path: &str| ChangedFileAt { untracked, path: path.into() };

    let first = ChangedFilesPage::after("c1".into(), files(), &at(false, "a.rs"), 2);
    let gone_from = ChangedFilesPage::after("c1".into(), files(), &at(false, "b.rs"), 2);
    let last = ChangedFilesPage::after("c1".into(), files(), &at(true, "a.rs"), 2);

    assert_eq!((paths(&first), first.left_out), (vec![at(false, "c.rs"), at(true, "a.rs")], 2));
    assert_eq!(
        (paths(&gone_from), gone_from.left_out),
        (vec![at(false, "c.rs"), at(true, "a.rs")], 2),
        "no b.rs tracked"
    );
    assert_eq!((paths(&last), last.left_out), (vec![at(true, "b.rs"), at(true, "d.rs")], 0));
    let summary = ChangeSummary::listing_at_most("c1".into(), files(), 2);
    let rest = ChangedFilesPage::after("c1".into(), files(), &summary.files[1].at(), 2);
    assert_eq!((paths(&rest), rest.left_out), (vec![at(true, "a.rs"), at(true, "b.rs")], 1), "on from the summary");
}
