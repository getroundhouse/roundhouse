//! A decoded frame's pixels, as the decoder holds them.

/// A frame as 8-bit planes of Y, U and V, as VP8 and VP9 store a frame: U and V at half the width and height, rounded
/// up, or, in VP9, at the full width or height or both, as its [`Subsampling`] says. One whose [`Matrix`] is
/// [`Matrix::Identity`] holds green, blue and red in those planes instead. It borrows the decoder's memory, so it's
/// drawn or copied before the next frame is decoded.
#[derive(Debug)]
pub struct Picture<'a> {
    /// The size it's shown at, in pixels.
    pub width: usize,
    pub height: usize,
    pub colors: Colors,
    pub subsampling: Subsampling,
    pub y: Plane<'a>,
    pub u: Plane<'a>,
    pub v: Plane<'a>,
}

/// How many of a picture's pixels share each U and V, as powers of two, as libvpx's chroma shifts say: 1 and 1 for
/// 4:2:0, where four pixels in a square share one, 1 and 0 for 4:2:2, where two side by side do, 0 and 1 for 4:4:0,
/// where two one over the other do, and 0 and 0 for 4:4:4, where each pixel has its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subsampling {
    pub across: u32,
    pub down: u32,
}

impl Subsampling {
    pub const FOUR_TWO_ZERO: Subsampling = Subsampling { across: 1, down: 1 };
    pub const FOUR_FOUR_FOUR: Subsampling = Subsampling { across: 0, down: 0 };

    /// How wide and tall U and V are for a picture `width` by `height`, each rounded up.
    pub fn chroma_size(self, width: usize, height: usize) -> (usize, usize) {
        (width.div_ceil(1 << self.across), height.div_ceil(1 << self.down))
    }
}

/// How a picture's Y, U and V become red, green and blue, as its stream says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub matrix: Matrix,
    /// Whether Y, U and V use every value from 0 to 255, rather than the studio's 16 to 235 and 240.
    pub full_range: bool,
}

/// The matrix that turns a picture's Y, U and V into red, green and blue: BT.601's, which VP8 always uses and VP9 does
/// unless it says otherwise, or BT.709's, which VP9 can say, as a recording of a high-definition screen may.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    Bt601,
    Bt709,
    /// None: the planes hold green, blue and red themselves, each pixel its own, using every value, as VP9 stores a
    /// video made from RGB frames, such as screenshots, unless it's told to turn them into Y, U and V.
    Identity,
}

/// One plane of a picture: `height` rows of `width` bytes, each `stride` bytes after the last.
#[derive(Debug)]
pub struct Plane<'a> {
    /// From the first byte of the first row to the last byte of the last, which the bytes past `width` in each other
    /// row are part of.
    pub data: &'a [u8],
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

impl Plane<'_> {
    /// Each row's `width` bytes, top to bottom, without what's past them.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        (0..self.height).map(|row| self.row(row))
    }

    /// Row `index`'s `width` bytes, counting from the top.
    pub fn row(&self, index: usize) -> &[u8] {
        &self.data[index * self.stride..index * self.stride + self.width]
    }
}
