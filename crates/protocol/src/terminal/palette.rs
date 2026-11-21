//! The colours of the palette that a picture leaves out, since the program didn't change them.

/// The red, green and blue of each of the 256 colours a program hasn't changed, as the worker's terminal has them:
/// Ghostty's own 16, then a cube of six levels of each, then 24 greys.
pub const DEFAULT_PALETTE: [[u8; 3]; 256] = default_palette();

const FIRST_16: [[u8; 3]; 16] = [
    [0x1d, 0x1f, 0x21],
    [0xcc, 0x66, 0x66],
    [0xb5, 0xbd, 0x68],
    [0xf0, 0xc6, 0x74],
    [0x81, 0xa2, 0xbe],
    [0xb2, 0x94, 0xbb],
    [0x8a, 0xbe, 0xb7],
    [0xc5, 0xc8, 0xc6],
    [0x66, 0x66, 0x66],
    [0xd5, 0x4e, 0x53],
    [0xb9, 0xca, 0x4a],
    [0xe7, 0xc5, 0x47],
    [0x7a, 0xa6, 0xda],
    [0xc3, 0x97, 0xd8],
    [0x70, 0xc0, 0xb1],
    [0xea, 0xea, 0xea],
];

const fn default_palette() -> [[u8; 3]; 256] {
    const LEVELS: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
    let mut palette = [[0; 3]; 256];
    let mut at = 0;
    while at < 16 {
        palette[at] = FIRST_16[at];
        at += 1;
    }
    while at < 232 {
        let cube = at - 16;
        palette[at] = [LEVELS[cube / 36], LEVELS[cube / 6 % 6], LEVELS[cube % 6]];
        at += 1;
    }
    while at < 256 {
        #[expect(clippy::cast_possible_truncation, reason = "at most 8 + 10 * 23, which is 238")]
        let grey = (8 + (at - 232) * 10) as u8;
        palette[at] = [grey, grey, grey];
        at += 1;
    }
    palette
}
