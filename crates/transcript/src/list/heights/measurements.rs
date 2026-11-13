use gpui_kit::{Pixels, px};

/// What a row counts as before any has been measured. About what a session's rows come to on average, most of them
/// showing nothing and a few of them screens long.
pub(super) const FIRST_GUESS: f64 = 48.;
/// The least a row that hasn't been measured counts as. A session can open on a run of rows that show nothing, and
/// with the average of those as the guess, a window would take every row of the session to fill.
pub(super) const LEAST_GUESS: f64 = 4.;

/// Each row's height, with the sums that find a place among them without adding them all up.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct Heights {
    /// The height each row was last measured at.
    pub(super) measured: Vec<Option<Pixels>>,
    /// Over the measured rows, in the shape of a Fenwick tree: `tree[i]` covers the `i & -i` rows that end at row
    /// `i`, counting from 1.
    pub(super) tree: Vec<Covered>,
}

/// What a run of rows comes to.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Covered {
    /// How many of them have been measured.
    pub(super) measured: usize,
    /// What those come to. In an `f64`, since it's added to and taken from for as long as the session is open.
    pub(super) height: f64,
}

impl Covered {
    pub(super) fn add(&mut self, other: Covered) {
        self.measured += other.measured;
        self.height += other.height;
    }

    /// What `rows` rows come to, of which these are the measured ones, with the rest at `guess` each.
    pub(super) fn tall(self, rows: usize, guess: f64) -> f64 {
        self.height + count(rows - self.measured) * guess
    }
}

impl Heights {
    pub(in super::super) fn rows(&self) -> usize {
        self.measured.len()
    }

    /// Makes it `rows` rows, keeping what's known of those that stay. Rows only come and go at the end, and each one
    /// that comes or goes costs no more than finding a place does, since rows come one at a time as an agent works.
    pub(in super::super) fn set_rows(&mut self, rows: usize) {
        self.measured.truncate(rows);
        self.tree.truncate(rows + 1);
        if self.tree.is_empty() {
            // The tree counts from 1.
            self.tree.push(Covered::default());
        }
        while self.measured.len() < rows {
            self.measured.push(None);
            // The new row's place in the tree covers the rows before it that its place takes in, which are already
            // summed in the places just below it.
            let at = self.measured.len();
            let (mut covered, mut below) = (Covered::default(), at - 1);
            while below > at - lowest_bit(at) {
                covered.add(self.tree[below]);
                below -= lowest_bit(below);
            }
            self.tree.push(covered);
        }
    }

    /// Notes what `row` was measured at.
    pub(in super::super) fn measure(&mut self, row: usize, height: Pixels) {
        let Some(known) = self.measured.get_mut(row) else { return };
        let was = known.replace(height);
        if was == Some(height) {
            return;
        }
        let newly = usize::from(was.is_none());
        let taller = height.to_f64() - was.map_or(0., Pixels::to_f64);
        let mut at = row + 1;
        while at < self.tree.len() {
            self.tree[at].measured += newly;
            self.tree[at].height += taller;
            at += lowest_bit(at);
        }
    }

    /// What a row that hasn't been measured counts as: what the measured ones come to on average.
    pub(in super::super) fn guess(&self) -> Pixels {
        narrow(self.guess_at())
    }

    pub(super) fn guess_at(&self) -> f64 {
        let all = self.before_row(self.rows());
        if all.measured == 0 { FIRST_GUESS } else { (all.height / count(all.measured)).max(LEAST_GUESS) }
    }

    /// What `row` was last measured at, if it has been.
    pub(in super::super) fn measured(&self, row: usize) -> Option<Pixels> {
        self.measured.get(row).copied().flatten()
    }

    /// How tall `row` is, or is guessed to be.
    pub(in super::super) fn height_of(&self, row: usize) -> Pixels {
        self.measured(row).unwrap_or_else(|| self.guess())
    }

    /// What the measured rows before `row` come to.
    pub(super) fn before_row(&self, row: usize) -> Covered {
        let (mut covered, mut at) = (Covered::default(), row.min(self.rows()));
        while at > 0 {
            covered.add(self.tree[at]);
            at -= lowest_bit(at);
        }
        covered
    }

    /// How far down the session `row` starts.
    pub(in super::super) fn above(&self, row: usize) -> Pixels {
        let row = row.min(self.rows());
        narrow(self.before_row(row).tall(row, self.guess_at()))
    }

    /// How tall the whole session is.
    pub(in super::super) fn total(&self) -> Pixels {
        self.above(self.rows())
    }

    /// The row that's `down` from the top of the session, and how far into that row. Past the end, it's the last
    /// row, and however far past its top that is.
    pub(in super::super) fn row_at(&self, down: Pixels) -> (usize, Pixels) {
        let (guess, down) = (self.guess_at(), down.max(px(0.)).to_f64());
        let rows = self.rows();
        // Down the tree, a power of two rows at a time, taking each run that ends at or before the place.
        let (mut row, mut passed) = (0, 0.);
        let mut step = rows.checked_next_power_of_two().unwrap_or(rows);
        while step > 0 {
            let next = row + step;
            if next <= rows {
                let through = passed + self.tree[next].tall(step, guess);
                if through <= down && next < rows {
                    (row, passed) = (next, through);
                }
            }
            step /= 2;
        }
        (row, narrow(down - passed))
    }
}

fn lowest_bit(at: usize) -> usize {
    at.isolate_lowest_one()
}

#[expect(clippy::cast_precision_loss, reason = "a count of rows, which is exact in an f64 far past any session")]
pub(super) fn count(rows: usize) -> f64 {
    rows as f64
}

/// A height on screen, which an `f32` holds to a fraction of a pixel.
pub(super) fn narrow(height: f64) -> Pixels {
    Pixels::from(height)
}
