use super::*;

fn file() -> (tempfile::TempDir, SettingsFile) {
    let dir = tempfile::tempdir().unwrap();
    let file = SettingsFile::in_dir(&dir.path().join("data"));
    (dir, file)
}

#[test]
fn what_is_saved_reads_back_the_same() {
    let (_dir, file) = file();
    let chosen = Settings { always_expand_thinking: true, sidebar_sort: Sort::Oldest, ..Settings::default() };

    file.write(&chosen).unwrap();

    assert_eq!(file.read(), chosen);
}

#[test]
fn nothing_saved_reads_as_nobody_has_chosen() {
    let (_dir, file) = file();

    assert_eq!(file.read(), Settings::default());
    assert!(!Settings::default().always_expand_thinking);
}

#[test]
fn notifications_and_their_sound_are_on_unless_turned_off() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    std::fs::write(&file.path, r#"{"always_expand_thinking": true}"#).unwrap();

    let settings = file.read();

    assert!(settings.notify && settings.play_sound);
    assert_eq!(settings.sound, Sound::Chime);
}

#[test]
fn a_sound_a_newer_build_chose_reads_as_chime_and_the_rest_still_reads() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    std::fs::write(&file.path, r#"{"notify": false, "sound": {"bundled": "whistle"}}"#).unwrap();

    let settings = file.read();

    assert!(!settings.notify);
    assert_eq!(settings.sound, Sound::Chime);
}

#[test]
fn a_file_that_cant_be_read_counts_as_nothing_saved() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    std::fs::write(&file.path, "{ not json").unwrap();

    assert_eq!(file.read(), Settings::default());
}

#[test]
fn saving_keeps_what_a_newer_build_wrote() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    std::fs::write(&file.path, r#"{"always_expand_thinking": false, "from_later": [1, 2]}"#).unwrap();

    let mut settings = file.read();
    settings.always_expand_thinking = true;
    file.write(&settings).unwrap();

    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap()).unwrap();
    assert_eq!(
        saved,
        serde_json::json!({
            "always_expand_thinking": true,
            "sidebar_sort": "last_active",
            "notify": true,
            "play_sound": true,
            "sound": "chime",
            "from_later": [1, 2],
        })
    );
}

#[test]
fn an_order_for_the_sidebar_that_a_newer_build_chose_reads_as_last_active_and_the_rest_still_reads() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    std::fs::write(&file.path, r#"{"always_expand_thinking": true, "sidebar_sort": "by_colour"}"#).unwrap();

    let settings = file.read();

    assert!(settings.always_expand_thinking);
    assert_eq!(settings.sidebar_sort, Sort::LastActive);
}

#[test]
fn saving_leaves_nothing_else_in_the_directory() {
    let (_dir, file) = file();

    file.write(&Settings::default()).unwrap();

    let names: Vec<_> =
        std::fs::read_dir(file.path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["settings.json"]);
}
