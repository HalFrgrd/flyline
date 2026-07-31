use crate::active_suggestions::ANIMATION_FRAME_FPS;
use crate::content::Coord;
use crate::settings::ColourTheme;
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

/// Fade-target background RGB for the given colour theme.
pub fn theme_fade_bg(theme: ColourTheme) -> (u8, u8, u8) {
    match theme {
        ColourTheme::Dark => (0, 0, 0),
        ColourTheme::Light => (255, 255, 255),
    }
}

/// Full-intensity default cursor RGB: high contrast against the theme background.
pub fn theme_default_cursor_rgb(theme: ColourTheme) -> (u8, u8, u8) {
    match theme {
        ColourTheme::Dark => (255, 255, 255),
        ColourTheme::Light => (0, 0, 0),
    }
}

/// Linearly interpolate between two RGB colours by `t` ∈ [0, 1].
fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let r = (from.0 as f32 + (to.0 as f32 - from.0 as f32) * t) as u8;
    let g = (from.1 as f32 + (to.1 as f32 - from.1 as f32) * t) as u8;
    let b = (from.2 as f32 + (to.2 as f32 - from.2 as f32) * t) as u8;
    Color::Rgb(r, g, b)
}

/// Which backend renders the cursor.
#[derive(
    ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum CursorBackend {
    /// Flyline renders a custom cursor.
    #[default]
    Flyline,
    /// Leave cursor rendering entirely to the terminal emulator.
    Terminal,
}

/// Map a normalised intensity ∈ [0.2, 1.0] to an Rgb colour by lerping from the
/// theme fade background toward the high-contrast default cursor colour.
fn intensity_to_rgb(intensity: f32, theme: ColourTheme) -> Color {
    lerp_rgb(
        theme_fade_bg(theme),
        theme_default_cursor_rgb(theme),
        intensity,
    )
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
///
/// Uses the dark theme colours (white cursor fading toward black) for the CLI
/// completion preview.
pub fn cursor_effect_animation_frames(
    easing: CursorEasing,
    effect_speed: f32,
) -> Vec<Vec<Span<'static>>> {
    let total_frames = cursor_effect_total_frames(effect_speed);
    let mut frames = Vec::with_capacity(total_frames);

    let make_frame = |intensity: f32| -> Vec<Span<'static>> {
        vec![Span::styled(
            " ",
            Style::new().bg(intensity_to_rgb(intensity, ColourTheme::Dark)),
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
#[derive(
    ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

    /// Return the cursor style based on the config, focus state, and colour theme.
    ///
    /// Returns `None` if the cursor should be hidden (e.g. blink off-phase).
    /// When `focused` is false the cursor is rendered at a steady dim level.
    pub fn get_style(
        &self,
        focused: bool,
        config: &CursorConfig,
        selection_bg: Option<Color>,
        selection_active: bool,
        theme: ColourTheme,
    ) -> Option<Style> {
        let intensity = if selection_active {
            1.0
        } else {
            self.compute_intensity(focused, config)?
        };
        Some(Self::build_style(
            intensity,
            &config.style,
            selection_bg,
            theme,
        ))
    }

    /// Build a cursor style for a static (non-animated) intensity level.
    ///
    /// `intensity` is a raw 0–255 value (e.g. [`CURSOR_INTENSITY_UNFOCUSED`] or 255).
    pub fn static_style(
        intensity: u8,
        style_config: &CursorStyleConfig,
        theme: ColourTheme,
    ) -> Style {
        Self::build_style(intensity as f32 / 255.0, style_config, None, theme)
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
    ///
    /// Intensity fades by lerping from the theme background toward the cursor
    /// colour, so dim phases wash out into the background on both light and dark
    /// themes instead of multiplying toward black.
    fn build_style(
        intensity: f32,
        style_config: &CursorStyleConfig,
        selection_bg: Option<Color>,
        theme: ColourTheme,
    ) -> Style {
        let selection_rgb = match selection_bg {
            Some(Color::Rgb(r, g, b)) => Some((r, g, b)),
            _ => None,
        };
        let fade_bg = theme_fade_bg(theme);

        match style_config {
            CursorStyleConfig::Default => {
                if let Some((sr, sg, sb)) = selection_rgb {
                    let cursor = theme_default_cursor_rgb(theme);
                    Style::new().bg(lerp_rgb((sr, sg, sb), cursor, intensity))
                } else {
                    Style::new().bg(lerp_rgb(
                        fade_bg,
                        theme_default_cursor_rgb(theme),
                        intensity,
                    ))
                }
            }
            CursorStyleConfig::Reverse => Style::new().add_modifier(Modifier::REVERSED),
            CursorStyleConfig::Custom(style) => {
                let bg = match style.bg {
                    Some(Color::Rgb(r, g, b)) => {
                        if let Some((sr, sg, sb)) = selection_rgb {
                            Some(lerp_rgb((sr, sg, sb), (r, g, b), intensity))
                        } else {
                            Some(lerp_rgb(fade_bg, (r, g, b), intensity))
                        }
                    }
                    other => other,
                };
                Style { bg, ..*style }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bg_rgb(style: Style) -> (u8, u8, u8) {
        match style.bg {
            Some(Color::Rgb(r, g, b)) => (r, g, b),
            other => panic!("expected Rgb bg, got {:?}", other),
        }
    }

    #[test]
    fn dark_default_full_intensity_is_white() {
        let style = Cursor::static_style(255, &CursorStyleConfig::Default, ColourTheme::Dark);
        assert_eq!(bg_rgb(style), (255, 255, 255));
    }

    #[test]
    fn dark_default_dim_is_near_black() {
        // 51/255 ≈ 0.2, the fade floor
        let style = Cursor::static_style(51, &CursorStyleConfig::Default, ColourTheme::Dark);
        assert_eq!(bg_rgb(style), (51, 51, 51));
    }

    #[test]
    fn light_default_full_intensity_is_black() {
        let style = Cursor::static_style(255, &CursorStyleConfig::Default, ColourTheme::Light);
        assert_eq!(bg_rgb(style), (0, 0, 0));
    }

    #[test]
    fn light_default_dim_is_near_white() {
        // 51/255 ≈ 0.2 → lerp(white, black, 0.2) ≈ (204, 204, 204)
        let style = Cursor::static_style(51, &CursorStyleConfig::Default, ColourTheme::Light);
        assert_eq!(bg_rgb(style), (204, 204, 204));
    }

    #[test]
    fn light_custom_rgb_dim_washes_toward_white() {
        let custom = CursorStyleConfig::Custom(Style::new().bg(Color::Rgb(0, 100, 200)));
        let style = Cursor::static_style(51, &custom, ColourTheme::Light);
        // lerp((255,255,255), (0,100,200), 51/255) — washed toward white, not black
        let (r, g, b) = bg_rgb(style);
        assert!(r > 200, "red should wash toward white, got {r}");
        assert!(g > 200, "green should wash toward white, got {g}");
        assert!(b > 200, "blue should wash toward white, got {b}");
        // Must not be the old multiply-toward-black result (~(0, 20, 40))
        assert_ne!((r, g, b), (0, 20, 40));
    }

    #[test]
    fn light_custom_rgb_full_keeps_color() {
        let custom = CursorStyleConfig::Custom(Style::new().bg(Color::Rgb(0, 100, 200)));
        let style = Cursor::static_style(255, &custom, ColourTheme::Light);
        assert_eq!(bg_rgb(style), (0, 100, 200));
    }
}
