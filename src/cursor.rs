use crate::content_builder::Coord;
use crate::settings::{CursorConfig, CursorEffect, CursorStyleConfig};
use ratatui::style::{Color, Modifier, Style};
use std::time::Instant;

/// Cursor intensity used when the terminal has lost focus (or in modes where
/// the cursor should appear dimmed without animation).
pub const CURSOR_INTENSITY_UNFOCUSED: u8 = 80;

pub struct Cursor {
    target_pos: Coord,
    prev_pos: Coord,
    time_of_change: Instant,
}

impl Cursor {
    pub fn new() -> Self {
        let now = Instant::now();
        Cursor {
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

    /// Return the (possibly interpolated) cursor position based on the given config.
    pub fn get_position(&self, config: &CursorConfig) -> Coord {
        match config.interpolate {
            None => self.target_pos,
            Some(speed) => {
                let time_since_change = self.time_of_change.elapsed().as_secs_f32();
                let mut factor = time_since_change * speed + 0.2;

                // Adjust factor for small movements
                if self.prev_pos.abs_diff(&self.target_pos) <= 2 {
                    factor = 1.0;
                }

                let t = factor.min(1.0);
                let eased_t = config.interpolate_easing.apply(t);
                self.prev_pos.interpolate(&self.target_pos, eased_t)
            }
        }
    }

    /// Return the cursor style based on the config and focus state.
    ///
    /// Returns `None` if the cursor should be hidden (e.g. blink off-phase).
    /// When `focused` is false the cursor is rendered at a steady dim level.
    pub fn get_style(&self, focused: bool, config: &CursorConfig) -> Option<Style> {
        let intensity = self.compute_intensity(focused, config)?;
        Some(Self::build_style(intensity, &config.style))
    }

    /// Compute a normalised intensity ∈ [0, 1] for the current effect phase.
    /// Returns `None` when the cursor should be fully hidden (blink off-phase).
    fn compute_intensity(&self, focused: bool, config: &CursorConfig) -> Option<f32> {
        if !focused {
            return Some(CURSOR_INTENSITY_UNFOCUSED as f32 / 255.0);
        }

        match config.effect {
            CursorEffect::None => Some(1.0),
            CursorEffect::Fade => {
                let elapsed = self.time_of_change.elapsed().as_secs_f32();
                // Raw value in [0, 1] from a sine wave, scaled by effect_speed.
                let raw = (elapsed * 4.0 * config.effect_speed).sin() * 0.5 + 0.5;
                let eased = config.effect_easing.apply(raw.clamp(0.0, 1.0));
                // Map eased [0, 1] → [0.2, 1.0] so the cursor never fully disappears.
                Some(eased * 0.8 + 0.2)
            }
            CursorEffect::Blink => {
                let elapsed = self.time_of_change.elapsed().as_secs_f32();
                let phase = (elapsed * config.effect_speed).fract();
                if phase < 0.5 { Some(1.0) } else { None }
            }
        }
    }

    /// Build a ratatui `Style` from a normalised intensity and the cursor style config.
    fn build_style(intensity: f32, style_config: &CursorStyleConfig) -> Style {
        match style_config {
            CursorStyleConfig::Default => {
                let v = (intensity * 255.0) as u8;
                Style::new().bg(Color::Rgb(v, v, v))
            }
            CursorStyleConfig::Reverse => Style::new().add_modifier(Modifier::REVERSED),
            CursorStyleConfig::Custom(style) => *style,
        }
    }
}
