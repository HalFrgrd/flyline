use crate::active_suggestions::ANIMATION_FRAME_FPS;
use crate::content_builder::Coord;
use clap::ValueEnum;
use easing_function::Easing as _;
use easing_function::easings::StandardEasing;
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

/// Easing function used for cursor position interpolation or visual effects.
///
/// Corresponds to the standard easings from the `easing-function` crate:
/// <https://docs.rs/easing-function/latest/easing_function/easings/index.html>
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorEasing {
    #[default]
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InCirc,
    OutCirc,
    InOutCirc,
    InExpo,
    OutExpo,
    InOutExpo,
    InElastic,
    OutElastic,
    InOutElastic,
    InBack,
    OutBack,
    InOutBack,
    InBounce,
    OutBounce,
    InOutBounce,
}

impl CursorEasing {
    /// Try to parse a string as a `CursorEasing` value using clap's case-insensitive
    /// value-name matching (e.g. `"in-quad"` → `CursorEasing::InQuad`).
    pub fn try_from_value_name(s: &str) -> Option<Self> {
        <Self as clap::ValueEnum>::from_str(s, true).ok()
    }

    /// Apply the easing function to `t` ∈ [0, 1], returning a value in [0, 1].
    pub fn apply(self, t: f32) -> f32 {
        match self {
            CursorEasing::Linear => StandardEasing::Linear.ease(t),
            CursorEasing::InQuad => StandardEasing::InQuadradic.ease(t),
            CursorEasing::OutQuad => StandardEasing::OutQuadradic.ease(t),
            CursorEasing::InOutQuad => StandardEasing::InOutQuadradic.ease(t),
            CursorEasing::InCubic => StandardEasing::InCubic.ease(t),
            CursorEasing::OutCubic => StandardEasing::OutCubic.ease(t),
            CursorEasing::InOutCubic => StandardEasing::InOutCubic.ease(t),
            CursorEasing::InQuart => StandardEasing::InQuartic.ease(t),
            CursorEasing::OutQuart => StandardEasing::OutQuartic.ease(t),
            CursorEasing::InOutQuart => StandardEasing::InOutQuartic.ease(t),
            CursorEasing::InQuint => StandardEasing::InQuintic.ease(t),
            CursorEasing::OutQuint => StandardEasing::OutQuintic.ease(t),
            CursorEasing::InOutQuint => StandardEasing::InOutQuintic.ease(t),
            CursorEasing::InSine => StandardEasing::InSine.ease(t),
            CursorEasing::OutSine => StandardEasing::OutSine.ease(t),
            CursorEasing::InOutSine => StandardEasing::InOutSine.ease(t),
            CursorEasing::InCirc => StandardEasing::InCircular.ease(t),
            CursorEasing::OutCirc => StandardEasing::OutCircular.ease(t),
            CursorEasing::InOutCirc => StandardEasing::InOutCircular.ease(t),
            CursorEasing::InExpo => StandardEasing::InExponential.ease(t),
            CursorEasing::OutExpo => StandardEasing::OutExponential.ease(t),
            CursorEasing::InOutExpo => StandardEasing::InOutExponential.ease(t),
            CursorEasing::InElastic => StandardEasing::InElastic.ease(t),
            CursorEasing::OutElastic => StandardEasing::OutElastic.ease(t),
            CursorEasing::InOutElastic => StandardEasing::InOutElastic.ease(t),
            CursorEasing::InBack => StandardEasing::InBack.ease(t),
            CursorEasing::OutBack => StandardEasing::OutBack.ease(t),
            CursorEasing::InOutBack => StandardEasing::InOutBack.ease(t),
            CursorEasing::InBounce => StandardEasing::InBounce.ease(t),
            CursorEasing::OutBounce => StandardEasing::OutBounce.ease(t),
            CursorEasing::InOutBounce => StandardEasing::InOutBounce.ease(t),
        }
    }
}

/// Compute the cursor intensity for the fade effect from a raw oscillation value
/// `raw_t` ∈ [0, 1] (e.g. the output of a sine wave) and the given easing function.
///
/// Maps the eased value to [0.2, 1.0] so the cursor is always at least faintly visible.
pub fn fade_intensity(raw_t: f32, easing: CursorEasing) -> f32 {
    let eased = easing.apply(raw_t.clamp(0.0, 1.0));
    // Map eased [0, 1] → [0.2, 1.0] so the cursor never fully disappears.
    eased * 0.8 + 0.2
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

/// How the cursor should be styled.
#[derive(Debug, Clone, PartialEq)]
pub enum CursorStyleConfig {
    /// Default: an intensity-modulated grey/white block (original flyline cursor).
    Default,
    /// Reverse the colours of the cell under the cursor.
    Reverse,
    /// Apply a custom ratatui style.  A single colour (no `on`) is treated as
    /// the background colour; `"pink on white"` → fg=pink, bg=white.
    Custom(ratatui::style::Style),
}

impl Default for CursorStyleConfig {
    fn default() -> Self {
        CursorStyleConfig::Default
    }
}

/// Complete cursor configuration set by `flyline set-cursor`.
#[derive(Debug, Clone)]
pub struct CursorConfig {
    /// Which backend renders the cursor.
    pub backend: CursorBackend,
    /// Interpolation speed (cells per second).  `None` disables position
    /// interpolation and the cursor jumps instantly to its target.
    /// Default is `Some(200.0)`.
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

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            backend: CursorBackend::Flyline,
            interpolate: Some(200.0),
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
                let distance = self.prev_pos.abs_diff(&self.target_pos) as f32;

                // Snap small movements instantly to avoid sub-frame jitter.
                if distance <= 2.0 {
                    return self.target_pos;
                }

                // speed is in cells per second: the animation completes after
                // (distance / speed) seconds, so factor reaches 1.0 exactly when
                // the cursor has "travelled" the full distance at the given speed.
                let time_since_change = self.time_of_change.elapsed().as_secs_f32();
                let factor = time_since_change * speed / distance;

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
    fn build_style(intensity: f32, style_config: &CursorStyleConfig) -> Style {
        match style_config {
            CursorStyleConfig::Default => {
                let v = (intensity * 255.0) as u8;
                Style::new().bg(Color::Rgb(v, v, v))
            }
            CursorStyleConfig::Reverse => Style::new().add_modifier(Modifier::REVERSED),
            CursorStyleConfig::Custom(style) => {
                // For Rgb background colours, modulate the brightness by intensity so
                // that Fade/Blink effects are visually consistent with the Default style.
                // Named/indexed colours cannot be scaled, so they are used as-is.
                let bg = match style.bg {
                    Some(Color::Rgb(r, g, b)) => Some(Color::Rgb(
                        (r as f32 * intensity) as u8,
                        (g as f32 * intensity) as u8,
                        (b as f32 * intensity) as u8,
                    )),
                    other => other,
                };
                Style { bg, ..*style }
            }
        }
    }
}
