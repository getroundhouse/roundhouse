use protocol::terminal::TerminalSize;

use super::*;

const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };

/// Both limits, each where it's the one reached: the lines at a width people use, and the bytes in the widest
/// terminal, with a colour in every cell. Ghostty drops history a page at a time, so each is reached only roughly.
#[test]
fn a_terminal_keeps_its_lines_of_history_unless_they_take_too_much() {
    let mut narrow = Mirror::new(TerminalSize { cols: 200, rows: 50 }).unwrap();
    for line in 0..12_000 {
        narrow.write(format!("\x1b[32mline {line}\x1b[0m of a build's output, as long as most are\r\n").as_bytes());
    }
    let wide = TerminalSize::MOST;
    let mut colourful = Mirror::new(wide).unwrap();
    // About 41 MiB of them.
    for line in 0..8_000_usize {
        let cells = (0..usize::from(wide.cols)).map(|col| format!("\x1b[38;5;{}mx", (line + col) % 256));
        colourful.write(format!("{}\r\n", cells.collect::<String>()).as_bytes());
    }

    let rows = |mirror: &Mirror| mirror.terminal.scrollback_rows().unwrap();
    let bytes = |mirror: &Mirror| usize::try_from(mirror.terminal.memory_usage().unwrap().primary_resident_bytes);
    assert!(rows(&narrow) > HISTORY_LINES * 9 / 10, "{} rows", rows(&narrow));
    let colourful_bytes = bytes(&colourful).unwrap();
    assert!((HISTORY_BYTES * 9 / 10..HISTORY_BYTES * 11 / 10).contains(&colourful_bytes), "{colourful_bytes} bytes");
}

/// What the worker's copy answers, as a terminal does, since a program such as a shell waits for its answers.
fn answers(write: impl FnOnce(&mut Mirror)) -> Vec<u8> {
    let answered = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut worker = Mirror::answering(SIZE, {
        let answered = answered.clone();
        move |bytes| answered.borrow_mut().extend_from_slice(bytes)
    })
    .unwrap();
    write(&mut worker);
    drop(worker);
    std::rc::Rc::into_inner(answered).unwrap().into_inner()
}

#[test]
fn the_workers_copy_says_where_the_cursor_is_and_what_the_terminal_is() {
    assert_eq!(answers(|worker| worker.write(b"ab\x1b[6n")), b"\x1b[1;3R");
    assert_eq!(answers(|worker| worker.write(b"\x1b[c")), b"\x1b[?62;22c");
}

/// Since a picture has no images, as `Mirror::new` says. A program asks by sending an image, with `a=q`.
#[test]
fn a_program_that_asks_whether_it_can_show_images_is_told_nothing() {
    assert_eq!(answers(|worker| worker.write(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\")), b"");
}

#[test]
fn a_new_size_is_the_one_a_program_is_told_of() {
    let answered = answers(|worker| {
        worker.resize(TerminalSize { cols: 40, rows: 10 }).unwrap();
        worker.write(b"\x1b[99;99H\x1b[6n");
    });

    assert_eq!(answered, b"\x1b[10;40R");
}
