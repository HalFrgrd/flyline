use std::time::Instant;

pub struct CursorAnimation {
    target_pos: (usize, usize),
    prev_target_pos: (usize, usize),
    time_of_change: Instant,
    start_time: Instant,
}

impl CursorAnimation {
    pub fn new() -> Self {
        let now = Instant::now();
        CursorAnimation {
            target_pos: (0, 0),
            prev_target_pos: (0, 0),
            time_of_change: now,
            start_time: now,
        }
    }

    pub fn update_position(&mut self, new_pos: (usize, usize)) {
        if new_pos != self.target_pos {
            self.time_of_change = Instant::now();
            self.prev_target_pos = self.target_pos;
            self.target_pos = new_pos;
        }
    }

    pub fn get_position(&self) -> (usize, usize) {
        // interpolate between prevPos and currentPos based on time since time_of_change
        let time_since_change = self.time_of_change.elapsed().as_secs_f32();
        let mut factor = (time_since_change * 8.0).min(1.0); // Adjust speed here (8.0 = 125ms for full transition)

        if (self.prev_target_pos.0.abs_diff(self.target_pos.0)
            + self.prev_target_pos.1.abs_diff(self.target_pos.1))
            < 2
        {
            factor = 1.0;
        }

        let x = self.prev_target_pos.0 as f32
            + (self.target_pos.0 as f32 - self.prev_target_pos.0 as f32) * factor;
        let y = self.prev_target_pos.1 as f32
            + (self.target_pos.1 as f32 - self.prev_target_pos.1 as f32) * factor;
        (x as usize, y as usize)
    }

    pub fn get_intensity(&self) -> u8 {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let intensity_f32 = (elapsed * 4.0).sin() * 0.4 + 0.6;
        let intensity = (intensity_f32 * 255.0) as u8;
        intensity
    }
}
