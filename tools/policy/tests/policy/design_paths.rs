//! The paths the design's files name each other by. A copy of a component names its master's canvas by path, as
//! `canvas(Board/Status)`, and a film names the canvases its states are on. Brilliant points both at a canvas it moves
//! itself, but a canvas added on a branch from before the move still names the old path once they're merged. A copy
//! whose canvas isn't there draws nothing, and nothing says so, so a button could go missing from a screen unseen.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn design() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design")
}

/// The canvases that `text` names by path, in each `canvas(…)`.
fn named_canvases(text: &str) -> Vec<String> {
    let after = text.split("canvas(").skip(1);
    after.filter_map(|after| after.split_once(')')).map(|(path, _)| path.to_string()).collect()
}

/// The canvases a film's `canvases["…", "…"]` names, each by its name or its path.
fn a_films_canvases(film: &str) -> Vec<String> {
    let listed = film.split_once("canvases[").and_then(|(_, rest)| rest.split_once(']'));
    let names = listed.map(|(names, _)| names.split('"').skip(1).step_by(2).map(String::from).collect());
    names.unwrap_or_default()
}

/// Whether a film in `film` can play the canvas it names `name`: Brilliant looks for it by path from the design, and in
/// the folder that the film's `Motion/` is in.
fn found(name: &str, film: &Path, canvases: &BTreeSet<String>) -> bool {
    let beside = film.parent().and_then(Path::parent).map(|folder| folder.join(name));
    let beside = beside
        .map(|path| path.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
    canvases.contains(name) || beside.is_some_and(|path| canvases.contains(&path))
}

#[test]
fn a_copys_canvas_is_named_by_path_and_a_films_by_name_or_path() {
    let canvases: BTreeSet<String> = ["Board/Status", "Screens/Chats/Plan"].map(String::from).into();
    let film = Path::new("Screens/Chats/Motion/Opens.bm");

    assert_eq!(named_canvases("1 inst(\"Status Ready\",canvas(Board/Status)) \"Status\""), ["Board/Status"]);
    assert_eq!(
        a_films_canvases("fps(60)\ncanvases[\"Plan\", \"Screens/Chats/Plan\"]\n"),
        ["Plan", "Screens/Chats/Plan"]
    );
    assert!(found("Plan", film, &canvases));
    assert!(found("Screens/Chats/Plan", film, &canvases));
    assert!(!found("Plan", Path::new("Motion/Opens.bm"), &canvases));
}

#[test]
fn every_canvas_that_a_canvas_or_a_film_names_is_there() {
    let design = design();
    let canvases = design::canvases(&design).expect("the design's canvases");
    let films = design::films(&design).expect("the design's films");
    let there: BTreeSet<String> = canvases.iter().map(|canvas| canvas.path.to_string()).collect();

    let copies = canvases.iter().map(|canvas| (format!("{}.bl", canvas.path), &canvas.text));
    let in_states = films.iter().map(|film| (film.path.display().to_string(), &film.text));
    let mut missing: Vec<String> = copies
        .chain(in_states)
        .flat_map(|(file, text)| {
            let absent = named_canvases(text).into_iter().filter(|path| !there.contains(path));
            absent.map(move |path| format!("{file}: canvas({path})")).collect::<Vec<_>>()
        })
        .collect();
    for film in &films {
        let absent = a_films_canvases(&film.text).into_iter().filter(|name| !found(name, &film.path, &there));
        missing.extend(absent.map(|name| format!("{}: canvases[\"{name}\"]", film.path.display())));
    }

    assert!(
        missing.is_empty(),
        "these name a canvas the design doesn't have, as it's been moved or removed: {missing:#?}"
    );
}
