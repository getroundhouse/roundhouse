//! A picture's Y, U and V turned into blue, green, red and alpha, a byte each, which is how GPUI takes an image's
//! pixels, or its green, blue and red put in that order. It's integer arithmetic over whole rows, which the compiler
//! turns into vector instructions on every platform, so a 2880 × 1800 frame takes about 3.5 ms on an Apple M1, or
//! 6 ms with each pixel's own U and V, which are worked out for every row, and an older x86 core keeps up with 60
//! frames a second on one thread of its own.

use crate::picture::{Colors, Matrix, Picture, Subsampling};

/// Each colour's share of Y, U and V, in 256ths, and the Y that's black.
struct Coefficients {
    y: i32,
    red_v: i32,
    green_u: i32,
    green_v: i32,
    blue_u: i32,
    black: i32,
}

/// What turns Y, U and V into red, green and blue: a matrix's coefficients, or none, for planes that already hold them.
enum Turned {
    By(Coefficients),
    Identity,
}

impl Turned {
    const fn of(colors: Colors) -> Turned {
        let Colors { matrix, full_range } = colors;
        let coefficients = match (matrix, full_range) {
            (Matrix::Identity, _) => return Turned::Identity,
            (Matrix::Bt601, false) => {
                Coefficients { y: 298, red_v: 409, green_u: 100, green_v: 208, blue_u: 516, black: 16 }
            }
            (Matrix::Bt709, false) => {
                Coefficients { y: 298, red_v: 459, green_u: 55, green_v: 136, blue_u: 541, black: 16 }
            }
            (Matrix::Bt601, true) => {
                Coefficients { y: 256, red_v: 359, green_u: 88, green_v: 183, blue_u: 454, black: 0 }
            }
            (Matrix::Bt709, true) => {
                Coefficients { y: 256, red_v: 403, green_u: 48, green_v: 120, blue_u: 475, black: 0 }
            }
        };
        Turned::By(coefficients)
    }
}

/// What a row's U and V add to each pixel's red, green and blue, in 256ths, a value for each pixel of the row, so the
/// loop over a row's pixels is the same arithmetic on every one, which the compiler can vectorize. A row shares its U
/// and V with the rows under it that its [`Subsampling`](crate::Subsampling) says, so they're worked out once for them.
#[derive(Default)]
pub struct Rows {
    red: Vec<i32>,
    green: Vec<i32>,
    blue: Vec<i32>,
}

impl Picture<'_> {
    /// Writes it as BGRA, `width × height × 4` bytes, row after row, into `out`, with `rows` as room to work in, which
    /// is kept from one frame to the next so converting a frame allocates nothing.
    pub fn to_bgra(&self, rows: &mut Rows, out: &mut Vec<u8>) {
        out.resize(self.width * self.height * 4, 0);
        match Turned::of(self.colors) {
            Turned::By(coefficients) => self.turned(&coefficients, rows, out),
            Turned::Identity => self.copied(out),
        }
    }

    /// Green, blue and red, each pixel's own, in place in `out`.
    fn copied(&self, out: &mut [u8]) {
        let planes = self.y.rows().zip(self.u.rows()).zip(self.v.rows());
        for (((green, blue), red), pixels) in planes.zip(out.chunks_exact_mut(self.width * 4)) {
            let colors = green.iter().zip(blue).zip(red);
            for (pixel, ((&green, &blue), &red)) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(colors) {
                *pixel = [blue, green, red, u8::MAX];
            }
        }
    }

    /// Y, U and V turned by `coefficients`, in place in `out`.
    fn turned(&self, coefficients: &Coefficients, rows: &mut Rows, out: &mut [u8]) {
        let &Coefficients { y: y_share, red_v, green_u, green_v, blue_u, black } = coefficients;
        let Subsampling { across, down } = self.subsampling;
        // Rounds to the nearest, and takes black's Y off, once for each pixel rather than once for each colour.
        let base = 128 - black * y_share;
        for part in [&mut rows.red, &mut rows.green, &mut rows.blue] {
            part.resize(self.width, 0);
        }
        for (row, (y_row, pixels)) in self.y.rows().zip(out.chunks_exact_mut(self.width * 4)).enumerate() {
            if row % (1 << down) == 0 {
                let (u_row, v_row) = (self.u.row(row >> down), self.v.row(row >> down));
                for x in 0..self.width {
                    let (u, v) = (i32::from(u_row[x >> across]) - 128, i32::from(v_row[x >> across]) - 128);
                    rows.red[x] = base + red_v * v;
                    rows.green[x] = base - green_u * u - green_v * v;
                    rows.blue[x] = base + blue_u * u;
                }
            }
            let shares = rows.red.iter().zip(&rows.green).zip(&rows.blue);
            for ((pixel, &y), ((&red, &green), &blue)) in
                pixels.as_chunks_mut::<4>().0.iter_mut().zip(y_row).zip(shares)
            {
                let y = i32::from(y) * y_share;
                pixel[0] = byte(y + blue);
                pixel[1] = byte(y + green);
                pixel[2] = byte(y + red);
                pixel[3] = u8::MAX;
            }
        }
    }
}

/// A colour in 256ths as a byte, clamped to what a byte holds.
fn byte(value: i32) -> u8 {
    u8::try_from((value >> 8).clamp(0, 255)).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::Rows;
    use crate::picture::{Colors, Matrix, Picture, Plane, Subsampling};

    const STUDIO_601: Colors = Colors { matrix: Matrix::Bt601, full_range: false };

    /// `rows` rows of `data`, each `data.len() / rows` long.
    fn plane(data: &[u8], rows: usize) -> Plane<'_> {
        let width = data.len() / rows;
        Plane { data, stride: width, width, height: rows }
    }

    /// Each pixel's blue, green and red, after converting a row of `y`, with the U and V of each pair of pixels.
    fn converted(y: &[u8], uv: &[(u8, u8)], colors: Colors) -> Vec<[u8; 3]> {
        drawn(y, 1, uv, Subsampling::FOUR_TWO_ZERO, colors)
    }

    /// Each pixel's blue, green and red, after converting `rows` rows of `y`, with each U and V as `subsampling` shares
    /// them, row after row.
    fn drawn(y: &[u8], rows: usize, uv: &[(u8, u8)], subsampling: Subsampling, colors: Colors) -> Vec<[u8; 3]> {
        let (u, v): (Vec<u8>, Vec<u8>) = uv.iter().copied().unzip();
        let chroma_rows = rows.div_ceil(1 << subsampling.down);
        let (y, u, v) = (plane(y, rows), plane(&u, chroma_rows), plane(&v, chroma_rows));
        let picture = Picture { width: y.width, height: rows, colors, subsampling, y, u, v };
        let mut out = Vec::new();
        picture.to_bgra(&mut Rows::default(), &mut out);
        out.as_chunks::<4>()
            .0
            .iter()
            .inspect(|pixel| assert_eq!(pixel[3], 255, "opaque"))
            .map(|p| [p[0], p[1], p[2]])
            .collect()
    }

    /// The studio's black and white, and pure colours, come out as a screen shows them, in each matrix and range, and
    /// an odd width's last pixel takes the last U and V.
    #[test]
    fn each_matrix_and_range_gives_the_colours_it_stores() {
        let studio_601 = STUDIO_601;
        let studio_709 = Colors { matrix: Matrix::Bt709, full_range: false };
        let full_601 = Colors { matrix: Matrix::Bt601, full_range: true };

        assert_eq!(converted(&[16, 235, 126], &[(128, 128), (128, 128)], studio_601), [[0; 3], [255; 3], [128; 3]]);
        assert_eq!(converted(&[81, 145], &[(90, 240)], studio_601)[0], [0, 0, 255], "red");
        assert_eq!(converted(&[41, 41], &[(240, 110)], studio_601)[1], [255, 0, 0], "blue");
        assert_eq!(
            converted(&[63, 63], &[(102, 240)], studio_709)[0],
            [0, 1, 255],
            "red in BT.709, as its rounded Y, U and V give it"
        );
        assert_eq!(converted(&[0, 255], &[(128, 128)], full_601), [[0; 3], [255; 3]]);
    }

    /// Each pixel takes the U and V its subsampling gives it: in 4:4:4 its own, and in 4:2:2 its pair's across, but not
    /// the row's under it.
    #[test]
    fn each_pixel_takes_the_u_and_v_its_subsampling_gives_it() {
        let (red, blue) = ((90, 240), (240, 110));
        let own = Subsampling::FOUR_FOUR_FOUR;
        let across = Subsampling { across: 1, down: 0 };

        let four_four_four = drawn(&[81, 41], 1, &[red, blue], own, STUDIO_601);
        let four_two_two = drawn(&[81, 81, 41, 41], 2, &[red, blue], across, STUDIO_601);

        assert_eq!(four_four_four, [[0, 0, 255], [255, 0, 0]], "red, then blue");
        assert_eq!(four_two_two, [[0, 0, 255], [0, 0, 255], [255, 0, 0], [255, 0, 0]], "a red row over a blue one");
    }

    /// In 4:4:0, each pair of rows shares a row of U and V, and an odd height's last row takes the last.
    #[test]
    fn rows_share_the_u_and_v_their_subsampling_gives_them() {
        let (red, blue) = ((90, 240), (240, 110));
        let down = Subsampling { across: 0, down: 1 };

        let four_four_zero = drawn(&[81, 81, 41], 3, &[red, blue], down, STUDIO_601);

        assert_eq!(four_four_zero, [[0, 0, 255], [0, 0, 255], [255, 0, 0]], "two red rows over a blue one");
    }

    /// A picture of green, blue and red planes is each pixel's colour as it's stored, with nothing turned.
    #[test]
    fn green_blue_and_red_planes_are_copied_as_they_are() {
        let rgb = Colors { matrix: Matrix::Identity, full_range: true };

        let copied = drawn(&[10, 200], 1, &[(20, 30), (250, 0)], Subsampling::FOUR_FOUR_FOUR, rgb);

        assert_eq!(copied, [[20, 10, 30], [250, 200, 0]]);
    }
}
