use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Coord {
    x: usize,
    y: usize,
}

impl Coord {
    fn new(x: usize, y: usize) -> Self {
        Coord { x, y }
    }

    fn abs_diff(&self, other: &Coord) -> usize {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    fn interpolate(&self, other: &Coord, factor: f32) -> Coord {
        // factor = 0.0 => self
        // factor = 1.0 => other
        let x = self.x as f32 + (other.x as f32 - self.x as f32) * factor;
        let y = self.y as f32 + (other.y as f32 - self.y as f32) * factor;
        Coord::new(x as usize, y as usize)
    }

    fn to_tuple(&self) -> (usize, usize) {
        (self.x, self.y)
    }
}

pub struct CursorAnimation {
    target_pos: Coord,
    prev_pos: Coord,
    time_of_change: Instant,
}

impl CursorAnimation {
    pub fn new() -> Self {
        let now = Instant::now();
        CursorAnimation {
            target_pos: Coord::new(0, 0),
            prev_pos: Coord::new(0, 0),
            time_of_change: now,
        }
    }

    pub fn update_position(&mut self, new_pos: (usize, usize)) {
        let new_pos = Coord::new(new_pos.0, new_pos.1);
        if new_pos != self.target_pos {
            self.time_of_change = Instant::now();
            self.prev_pos = self.target_pos;
            self.target_pos = new_pos;
        }
    }

    pub fn get_position(&self) -> (usize, usize) {
        // interpolate between prevPos and currentPos based on time since time_of_change
        let time_since_change = self.time_of_change.elapsed().as_secs_f32();
        let mut factor = time_since_change * 16.0 + 0.2;

        // Adjust factor for small movements
        if self.prev_pos.abs_diff(&self.target_pos) <= 2 {
            factor = 1.0;
        }

        self.prev_pos
            .interpolate(&self.target_pos, factor.min(1.0))
            .to_tuple()
    }

    pub fn get_intensity(&self) -> u8 {
        // using time_of_change means the intensity is full right after movement
        let elapsed = self.time_of_change.elapsed().as_secs_f32();
        let intensity_f32 = (elapsed * 4.0).sin() * 0.4 + 0.6;
        let intensity = (intensity_f32 * 255.0) as u8;
        intensity
    }
}
