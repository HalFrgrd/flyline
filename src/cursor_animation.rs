use crate::content_builder::Coord;
use std::time::Instant;

/// Cursor intensity used when the terminal has lost focus (or in modes where
/// the cursor should appear dimmed without animation).
pub const CURSOR_INTENSITY_UNFOCUSED: u8 = 80;

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

    pub fn update_position(&mut self, new_pos: Coord) {
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

    pub fn get_position(&self) -> Coord {
        // interpolate between prevPos and currentPos based on time since time_of_change
        let time_since_change = self.time_of_change.elapsed().as_secs_f32();
        let mut factor = time_since_change * 16.0 + 0.2;

        // Adjust factor for small movements
        if self.prev_pos.abs_diff(&self.target_pos) <= 2 {
            factor = 1.0;
        }

        self.prev_pos.interpolate(&self.target_pos, factor.min(1.0))
    }

    /// Return the cursor intensity. When `focused` is false the cursor is
    /// rendered at [`CURSOR_INTENSITY_UNFOCUSED`] (a steady dim level) instead
    /// of the normal animated value.
    pub fn get_intensity(&self, focused: bool) -> u8 {
        if focused {
            // using time_of_change means the intensity is full right after movement
            let elapsed = self.time_of_change.elapsed().as_secs_f32();
            let intensity_f32 = (elapsed * 4.0).sin() * 0.4 + 0.6;

            (intensity_f32 * 255.0) as u8
        } else {
            CURSOR_INTENSITY_UNFOCUSED
        }
    }
}
