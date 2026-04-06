use std::collections::HashMap;

use crate::app::actions;
use crate::history::HistoryManager;
use crate::palette::Palette;
use clap::ValueEnum;
use easing_function::Easing as _;
use easing_function::easings::StandardEasing;

/// Which theme the user has configured for the colour palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorTheme {
    /// Dark-terminal preset (the original flyline palette). This is the default.
    #[default]
    Dark,
    /// Light-terminal preset.
    Light,
}

/// A single custom prompt animation registered with `flyline create-anim`.
#[derive(Debug, Clone)]
pub struct PromptAnimation {
    /// Name used as placeholder in prompt strings (e.g., `COOL_SPINNER`).
    pub name: String,
    /// Playback speed in frames per second.
    pub fps: f64,
    /// Animation frames.  May contain actual ANSI escape sequences (ESC byte, i.e. `\x1b`).
    pub frames: Vec<String>,
    /// When true the animation reverses direction at each end instead of
    /// wrapping around (ping-pong / bounce mode).
    pub ping_pong: bool,
}

/// A configured agent-mode command with its optional system prompt.
#[derive(Debug, Clone)]
pub struct AgentModeCommand {
    /// Command (and arguments) to invoke. The current buffer is appended as the
    /// final argument.
    pub command: Vec<String>,
    /// Optional system prompt prepended to the buffer when invoking AI mode.
    /// When set, the subprocess receives `"<system_prompt>\n<buffer>"` as its final argument.
    pub system_prompt: Option<String>,
}

/// Controls how flyline uses the terminal emulator's cursor.
#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq, Default)]
pub enum UseTermEmulatorCursor {
    /// Do not use the terminal emulator's cursor; flyline renders a custom cursor.
    None,
    /// Only send the escape codes that report the prompt start and end positions;
    /// flyline still renders a custom cursor for the active typing position.
    OnlyPromptPos,
    /// Fully use the terminal emulator's cursor: send prompt position codes and
    /// defer active cursor rendering to the terminal emulator. This is the default.
    #[default]
    Full,
}

/// Which backend renders the cursor.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorBackend {
    /// Flyline renders a custom cursor (default when using `flyline set-cursor`).
    #[default]
    Flyline,
    /// Leave cursor rendering entirely to the terminal emulator.
    Terminal,
}

/// Easing function used for cursor position interpolation or visual effects.
///
/// Corresponds to the standard easings from the `easing-function` crate:
/// <https://docs.rs/easing-function/latest/easing_function/easings/index.html>
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Visual effect applied to the cursor.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            backend: CursorBackend::Flyline,
            interpolate: Some(16.0),
            interpolate_easing: CursorEasing::Linear,
            style: CursorStyleConfig::Default,
            effect: CursorEffect::Fade,
            effect_speed: 1.0,
            effect_easing: CursorEasing::Linear,
        }
    }
}

/// Controls how flyline manages mouse capture.
#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq, Default)]
pub enum MouseMode {
    /// Never capture mouse events.
    Disabled,
    /// Mouse capture is on by default; toggled when Escape is pressed.
    Simple,
    /// Mouse capture is on by default with automatic management: disabled on scroll or when the
    /// user clicks above the viewport, re-enabled on any keypress or when focus is regained.
    #[default]
    Smart,
}

#[derive(Debug)]
pub struct Settings {
    /// Optional path to the Zsh history file. When `None`, Zsh history is not loaded.
    /// When `Some`, Zsh history is loaded in addition to Bash history; an empty string or no
    /// value means use the default path (`$HOME/.zsh_history`).
    pub zsh_history_path: Option<String>,
    /// Whether to show tutorial hints for first-time users.
    pub tutorial_mode: bool,
    /// Whether to show all animations (cursor movement, cursor fading, dynamic time).
    pub show_animations: bool,
    /// Whether to show inline history suggestions.
    pub show_inline_history: bool,
    /// Whether to automatically close opening characters (e.g., parentheses, brackets, quotes).
    pub auto_close_chars: bool,
    /// Whether to use the terminal emulator's cursor instead of rendering a custom cursor.
    pub use_term_emulator_cursor: UseTermEmulatorCursor,
    /// Cursor appearance and animation settings (set via `flyline set-cursor`).
    pub cursor_config: CursorConfig,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Agent-mode commands keyed by optional trigger prefix.
    /// - `None` key: the default command invoked via Alt+Enter (no prefix match needed).
    /// - `Some(prefix)` key: activated when the user presses Enter and the buffer starts
    ///   with `prefix`; the prefix is stripped before the buffer is sent to the command.
    pub agent_commands: HashMap<Option<String>, AgentModeCommand>,
    /// Custom prompt animations registered with `flyline create-anim`.
    pub custom_animations: HashMap<String, PromptAnimation>,
    /// Run matrix animation in the terminal background.
    pub matrix_animation: bool,
    /// Render frame rate in frames per second (1–120).
    pub frame_rate: u8,
    /// Whether to send shell integration escape codes (OSC 133 / OSC 633).
    pub send_shell_integration_codes: bool,
    /// Configurable colour palette for UI elements.
    pub color_palette: Palette,
    /// Which colour theme the user has selected (dark or light).
    pub color_theme: ColorTheme,
    /// User defined keybindings
    pub keybindings: Vec<actions::Binding>,
    /// User defined key remappings (applied before matching bindings).
    pub key_remappings: Vec<actions::KeyRemap>,
    /// Tracks commands that were cancelled via Ctrl+C (non-empty buffer).
    pub cancelled_command_history_manager: HistoryManager,
    /// Tracks prompts that were submitted to agent mode.
    pub agent_prompt_history_manager: HistoryManager,
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zsh_history_path: None,
            tutorial_mode: false,
            show_animations: true,
            show_inline_history: true,
            auto_close_chars: true,
            use_term_emulator_cursor: UseTermEmulatorCursor::Full,
            cursor_config: CursorConfig::default(),
            mouse_mode: MouseMode::Smart,
            agent_commands: HashMap::new(),
            custom_animations: HashMap::new(),
            matrix_animation: false,
            frame_rate: 30,
            send_shell_integration_codes: true,
            color_palette: Palette::default(),
            color_theme: ColorTheme::Dark,
            keybindings: Vec::new(),
            key_remappings: Vec::new(),
            cancelled_command_history_manager: HistoryManager::new_empty(),
            agent_prompt_history_manager: HistoryManager::new_empty(),
            #[cfg(feature = "integration-tests")]
            run_tab_completion_tests: false,
        }
    }
}
