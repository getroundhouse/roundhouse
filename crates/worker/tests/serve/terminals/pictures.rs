//! Pictures of what a terminal shows, which its worker sends while the hub watches it.

use std::collections::HashMap;
use std::time::Duration;

use protocol::terminal::{RowJson, TerminalPicture};
use tokio::time::Instant;

use super::hub::{Hub, SIZE};

fn text(row: &RowJson) -> String {
    row.row().spans.iter().map(|span| span.text.as_str()).collect()
}

fn rows_in(picture: &TerminalPicture) -> usize {
    picture.rows.iter().map(|run| run.rows.len()).sum()
}

/// The screen the hub has from `pictures`, as the text of each row, as of the newest whole one, from the rows sent in
/// its numbering.
fn screen(pictures: &[(TerminalPicture, Instant)]) -> Vec<String> {
    let Some(newest) = pictures.iter().rev().map(|(picture, _)| picture).find(|picture| picture.whole) else {
        return Vec::new();
    };
    let mut rows = HashMap::new();
    for (picture, _) in pictures.iter().filter(|(picture, _)| picture.numbering == newest.numbering) {
        for run in &picture.rows {
            rows.extend((run.from.get()..).zip(&run.rows));
        }
    }
    let numbers = newest.top.get()..newest.top.get() + u64::from(newest.size.rows);
    numbers.map(|number| rows.get(&number).map_or_else(String::new, |row| text(row))).collect()
}

/// The first picture is the whole screen, and those after it only the rows that changed, as typing changes a row at a
/// time on a screen that doesn't scroll.
#[tokio::test]
async fn a_watched_terminal_is_sent_its_screen_whole_and_then_what_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open_unwatched("t", dir.path()).await;
    hub.type_in("t", "echo be''fore").await;

    hub.watch("t", true).await;
    // The end of a row, since what the shell echoed of the first command can share the first line's row.
    hub.until_pictures(|pictures| screen(pictures).iter().any(|row| row.ends_with("before"))).await;
    hub.type_in("t", "echo af''ter").await;
    hub.until_pictures(|pictures| screen(pictures).iter().any(|row| row == "after")).await;

    let (first, after) = hub.pictures.split_first().unwrap();
    assert!(first.0.whole && first.0.cursor.is_some() && first.0.state.is_some(), "{first:?}");
    assert_eq!(rows_in(&first.0), usize::from(SIZE.rows));
    // The line typed, what it wrote, and the next prompt, any of which a machine's long name can wrap onto two rows.
    let few = usize::from(SIZE.rows) / 4;
    assert!(!after.is_empty() && after.iter().all(|(picture, _)| rows_in(picture) <= few), "{after:?}");
    hub.finish().await;
}

/// Nothing is sent of a terminal no hub watches, and once the hub stops watching, nothing of what comes after, until
/// it watches again, and is sent what the terminal shows by then.
#[tokio::test]
async fn a_terminal_is_sent_as_pictures_only_while_its_hub_watches_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open_unwatched("t", dir.path()).await;
    hub.type_in("t", "echo un''watched").await;
    // Longer than a picture takes to come, once it's due.
    hub.keep_for(Duration::from_millis(300)).await;
    let unwatched = hub.pictures.len();
    hub.watch("t", true).await;
    hub.until_shown("unwatched").await;

    hub.watch("t", false).await;
    hub.type_in("t", "echo un''watched again").await;
    hub.keep_for(Duration::from_millis(300)).await;
    let again = |(picture, _): &(TerminalPicture, Instant)| {
        picture.rows.iter().flat_map(|run| &run.rows).any(|row| text(row).contains("unwatched again"))
    };
    let sent_unwatched = hub.pictures.iter().any(again);
    hub.watch("t", true).await;

    assert_eq!(unwatched, 0);
    assert!(!sent_unwatched, "a picture came after the hub stopped watching");
    hub.until_shown("unwatched again").await;
    hub.finish().await;
}

/// What a program writes is taken in in full while the hub watches, however much it is, and the hub is sent where it
/// ended up.
#[tokio::test]
async fn after_a_flood_the_hub_is_sent_where_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;

    hub.type_in("t", "awk 'BEGIN{for(i=1;i<=200000;i++)print i}'; echo do''ne").await;

    until_screen_has(&mut hub, "done").await;
    let screen = screen(&hub.pictures);
    let done = screen.iter().position(|row| row == "done").unwrap();
    assert_eq!(screen[done - 1], "200000");
    hub.finish().await;
}

/// However often the program changes the screen, a picture begins at most every 16 ms, so a slow link is sent the
/// newest of what the terminal shows rather than all that came between. Small writes, each changing a row, which a
/// picture is quick to take, so only the pace holds them back.
#[tokio::test]
async fn pictures_come_at_most_about_60_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    let started = Instant::now();
    hub.open("t", dir.path()).await;

    let counts = "awk 'BEGIN{for(i=1;i<=3000;i++){printf \"%d\\r\", i; fflush(); for(j=0;j<20000;j++);}}'";
    hub.type_in("t", &format!("{counts}; echo do''ne")).await;
    until_screen_has(&mut hub, "done").await;

    let elapsed = started.elapsed();
    let pictures = hub.pictures.iter().filter(|(picture, _)| picture.whole).count();
    let most = usize::try_from(elapsed.as_millis() / 16).unwrap() + 2;
    assert!((2..=most).contains(&pictures), "{pictures} pictures in {elapsed:?}");
    hub.finish().await;
}

/// Not by the output, which a wait searches again for each frame that comes, and only when a picture has come.
async fn until_screen_has(hub: &mut Hub, text: &str) {
    let mut looked = 0;
    hub.until_pictures(|pictures| {
        let new = pictures.len() > looked;
        looked = pictures.len();
        new && screen(pictures).iter().any(|row| row == text)
    })
    .await;
}
