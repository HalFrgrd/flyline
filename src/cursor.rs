use crate::active_suggestions::ANIMATION_FRAME_FPS;
use crate::content::Coord;
use crate::term_info;
use clap::ValueEnum;
pub use flycontent::easing::{CursorEasing, fade_intensity};
pub use flycontent::palette::CursorStyleConfig;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::time::Instant;

/// Cursor intensity used when the terminal has lost focus (or in modes where
/// the cursor should appear dimmed without animation).
pub const CURSOR_INTENSITY_UNFOCUSED: u8 = 80;

/// Which backend renders the cursor.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorBackend {
    /// Flyline renders a custom cursor.
    #[default]
    Flyline,
    /// Leave cursor rendering entirely to the terminal emulator.
    Terminal,
}

/// Map a normalised intensity ∈ [0.2, 1.0] to an Rgb colour value scaled from
/// full white (255, 255, 255).
fn intensity_to_rgb(intensity: f32) -> Color {
    let v = (intensity * 255.0) as u8;
    Color::Rgb(v, v, v)
}

/// Angular speed constant used by the runtime fade effect.
const CURSOR_FADE_ANGULAR_SPEED: f32 = 4.0;

fn cursor_effect_total_frames(effect_speed: f32) -> usize {
    let cycle_duration_secs =
        std::f32::consts::TAU / (CURSOR_FADE_ANGULAR_SPEED * effect_speed.max(f32::EPSILON));
    (cycle_duration_secs * ANIMATION_FRAME_FPS as f32)
        .round()
        .max(2.0) as usize
}

/// Build animation frames that show a block cursor fading in and out using
/// `easing` to shape the intensity transition.
///
/// The preview is played back at `ANIMATION_FRAME_FPS`, so the frame count is
/// derived from the runtime fade period implied by `effect_speed`.
pub fn cursor_effect_animation_frames(
    easing: CursorEasing,
    effect_speed: f32,
) -> Vec<Vec<Span<'static>>> {
    let total_frames = cursor_effect_total_frames(effect_speed);
    let mut frames = Vec::with_capacity(total_frames);

    let make_frame = |intensity: f32| -> Vec<Span<'static>> {
        vec![Span::styled(
            " ",
            Style::new().bg(intensity_to_rgb(intensity)),
        )]
    };

    for i in 0..total_frames {
        let phase = i as f32 / total_frames as f32;
        let raw_t = if phase < 0.5 {
            phase * 2.0
        } else {
            (1.0 - phase) * 2.0
        };
        frames.push(make_frame(fade_intensity(raw_t, easing)));
    }

    frames
}

/// Visual effect applied to the cursor.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorEffect {
    /// Smoothly oscillate the cursor brightness (default).
    #[default]
    Fade,
    /// Hard on/off blinking.
    Blink,
    /// No effect; cursor is always shown at full brightness.
    None,
}

/// Complete cursor configuration set by `flyline set-cursor`.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorConfig {
    /// Which backend renders the cursor.  If `None`, the default is resolved
    /// dynamically based on terminal emulator checks.
    backend: Option<CursorBackend>,
    /// Interpolation speed.  `None` disables position
    /// interpolation and the cursor jumps instantly to its target.
    /// Default is `Some(16.0)`.
    pub interpolate: Option<f32>,
    /// Easing function applied to position interpolation.  Default: `Linear`.
    pub interpolate_easing: CursorEasing,
    /// Visual style of the cursor.  Default: `Default` (grey block).
    pub style: CursorStyleConfig,
    /// Visual effect applied to the cursor.  Default: `Fade`.
    pub effect: CursorEffect,
    /// Speed multiplier for the effect (1.0 = default rate).
    pub effect_speed: f32,
    /// Easing function applied to the effect intensity curve.  Default: `Linear`.
    pub effect_easing: CursorEasing,
}

impl CursorConfig {
    /// Resolves the cursor backend to use, defaulting to `Terminal` on Kitty and `Flyline` otherwise.
    pub fn backend(&self) -> CursorBackend {
        self.backend.unwrap_or_else(|| {
            if term_info::is_kitty() {
                CursorBackend::Terminal
            } else {
                CursorBackend::Flyline
            }
        })
    }

    /// Sets the cursor rendering backend.
    pub fn set_backend(&mut self, backend: Option<CursorBackend>) {
        self.backend = backend;
    }

    /// Returns `true` if no backend has been explicitly configured.
    pub fn is_backend_unset(&self) -> bool {
        self.backend.is_none()
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            backend: None,
            interpolate: Some(16.0),
            interpolate_easing: CursorEasing::Linear,
            style: CursorStyleConfig::Default,
            effect: CursorEffect::Fade,
            effect_speed: 1.0,
            effect_easing: CursorEasing::Linear,
        }
    }
}

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

    pub fn update_logical_pos(&mut self, new_pos: Coord) {
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
    pub fn get_render_pos(&self, config: &CursorConfig) -> Coord {
        match config.interpolate {
            None => self.target_pos,
            Some(speed) => {
                let time_since_change = self.time_of_change.elapsed().as_secs_f32();
                let mut factor = time_since_change * speed;

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
    pub fn get_style(
        &self,
        focused: bool,
        config: &CursorConfig,
        selection_bg: Option<Color>,
        selection_active: bool,
    ) -> Option<Style> {
        let intensity = if selection_active {
            1.0
        } else {
            self.compute_intensity(focused, config)?
        };
        Some(Self::build_style(intensity, &config.style, selection_bg))
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
                Some(fade_intensity(raw, config.effect_easing))
            }
            CursorEffect::Blink => {
                let elapsed = self.time_of_change.elapsed().as_secs_f32();
                let phase = (elapsed * config.effect_speed).fract();
                if phase < 0.5 { Some(1.0) } else { None }
            }
        }
    }

    /// Build a ratatui `Style` from a normalised intensity and the cursor style config.
    fn build_style(
        intensity: f32,
        style_config: &CursorStyleConfig,
        selection_bg: Option<Color>,
    ) -> Style {
        let selection_rgb = match selection_bg {
            Some(Color::Rgb(r, g, b)) => Some((r, g, b)),
            _ => None,
        };

        match style_config {
            CursorStyleConfig::Default => {
                if let Some((sr, sg, sb)) = selection_rgb {
                    let r = (sr as f32 + (255.0 - sr as f32) * intensity) as u8;
                    let g = (sg as f32 + (255.0 - sg as f32) * intensity) as u8;
                    let b = (sb as f32 + (255.0 - sb as f32) * intensity) as u8;
                    Style::new().bg(Color::Rgb(r, g, b))
                } else {
                    let v = (intensity * 255.0) as u8;
                    Style::new().bg(Color::Rgb(v, v, v))
                }
            }
            CursorStyleConfig::Reverse => Style::new().add_modifier(Modifier::REVERSED),
            CursorStyleConfig::Custom(style) => {
                let bg = match style.bg {
                    Some(Color::Rgb(r, g, b)) => {
                        if let Some((sr, sg, sb)) = selection_rgb {
                            let new_r = (sr as f32 + (r as f32 - sr as f32) * intensity) as u8;
                            let new_g = (sg as f32 + (g as f32 - sg as f32) * intensity) as u8;
                            let new_b = (sb as f32 + (b as f32 - sb as f32) * intensity) as u8;
                            Some(Color::Rgb(new_r, new_g, new_b))
                        } else {
                            Some(Color::Rgb(
                                (r as f32 * intensity) as u8,
                                (g as f32 * intensity) as u8,
                                (b as f32 * intensity) as u8,
                            ))
                        }
                    }
                    other => other,
                };
                Style { bg, ..*style }
            }
        }
    }
}
