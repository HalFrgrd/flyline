use crate::events;

pub struct CursorAnimation {
    target_pos: (u16, u16),
    Prev_target_pos: (u16, u16),
    tick_of_change: u64,
}

impl CursorAnimation {
    pub fn new() -> Self {
        CursorAnimation {
            target_pos: (0, 0),
            Prev_target_pos: (0, 0),
            tick_of_change: 0,
        }
    }

    pub fn update_position(&mut self, new_pos: (u16, u16), tick: u64) {
        if new_pos != self.target_pos {
            self.tick_of_change = tick;
            self.Prev_target_pos = self.target_pos;
            // self.Prev_target_pos = self.get_position(tick);
            self.target_pos = new_pos;
        }
    }

    pub fn get_position(&self, tick: u64) -> (u16, u16) {
        // interpolate between prevPos and currentPos based on time since tick_of_change
        let ticks_since_change = tick.saturating_sub(self.tick_of_change);
        let mut factor =
            (ticks_since_change as f32 * 0.008 * events::ANIMATION_TICK_RATE_MS as f32).min(1.0); // Adjust speed here
        if (self.Prev_target_pos.0.abs_diff(self.target_pos.0)
            + self.Prev_target_pos.1.abs_diff(self.target_pos.1))
            < 2
        {
            factor = 1.0;
        }
        let x = self.Prev_target_pos.0 as f32
            + (self.target_pos.0 as f32 - self.Prev_target_pos.0 as f32) * factor;
        let y = self.Prev_target_pos.1 as f32
            + (self.target_pos.1 as f32 - self.Prev_target_pos.1 as f32) * factor;
        (x as u16, y as u16)
    }

    pub fn get_intensity(&self, tick: u64) -> u8 {
        let mult = 0.004 * events::ANIMATION_TICK_RATE_MS as f32;
        let intensity_f32 = (tick as f32 * mult).sin() * 0.4 + 0.6;
        let intensity = (intensity_f32 * 255.0) as u8;
        intensity
        // 255
    }
}
