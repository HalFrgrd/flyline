use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Coord {
    row: u16,
    col: u16,
}

impl Coord {
    fn new(row: u16, col: u16) -> Self {
        Coord { row, col }
    }

    fn abs_diff(&self, other: &Coord) -> usize {
        self.col.abs_diff(other.col) as usize + self.row.abs_diff(other.row) as usize
    }

    fn interpolate(&self, other: &Coord, factor: f32) -> Coord {
        // factor = 0.0 => self
        // factor = 1.0 => other
        let col = self.col as f32 + (other.col as f32 - self.col as f32) * factor;
        let row = self.row as f32 + (other.row as f32 - self.row as f32) * factor;
        Coord::new(row as u16, col as u16)
    }

    fn to_tuple(&self) -> (u16, u16) {
        (self.row, self.col)
    }
}

pub struct CursorAnimation {
    target_pos: Coord,
    prev_pos: Coord,
    time_of_change: Instant,
    pub term_has_focus: bool,
}

impl CursorAnimation {
    pub fn new() -> Self {
        let now = Instant::now();
        CursorAnimation {
            target_pos: Coord::new(0, 0),
            prev_pos: Coord::new(0, 0),
            time_of_change: now,
            term_has_focus: true,
        }
    }

    pub fn update_position(&mut self, new_row: u16, new_col: u16) {
        let new_pos = Coord::new(new_row, new_col);
        if new_pos != self.target_pos {
            self.time_of_change = Instant::now();
            self.prev_pos = self.target_pos;
            if self.prev_pos == Coord::new(0, 0) {
                // First time setting position, no animation
                self.prev_pos = new_pos;
            }
            self.target_pos = new_pos;
        }
    }

    pub fn get_position(&self) -> (u16, u16) {
        // interpolate between prevPos and currentPos based on time since time_of_change
        let time_since_change = self.time_of_change.elapsed().as_secs_f32();
        let mut factor = time_since_change * 16.0 + 0.2;

        // Adjust factor for small movements
        if self.prev_pos.abs_diff(&self.target_pos) <= 2 {
            factor = 1.0;
        }

        let (interpolated_row, interpolated_col) = self
            .prev_pos
            .interpolate(&self.target_pos, factor.min(1.0))
            .to_tuple();
        (interpolated_row as u16, interpolated_col as u16)
    }

    pub fn get_intensity(&self) -> u8 {
        if self.term_has_focus {
            // using time_of_change means the intensity is full right after movement
            let elapsed = self.time_of_change.elapsed().as_secs_f32();
            let intensity_f32 = (elapsed * 4.0).sin() * 0.4 + 0.6;
            let intensity = (intensity_f32 * 255.0) as u8;
            intensity
        } else {
            80
        }
    }
}
