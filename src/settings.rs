use std::collections::HashMap;

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

/// Controls how flyline manages mouse capture.
#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq, Default)]
pub enum MouseMode {
    /// Never capture mouse events.
    Disabled,
    /// Mouse capture is on by default; toggled when Escape is pressed or Alt is pressed/released.
    Simple,
    /// Mouse capture is on by default with automatic management: disabled on scroll or when the
    /// mouse moves above the viewport, re-enabled on any keypress or when focus is regained.
    #[default]
    Smart,
}

#[derive(Debug, Default)]
pub struct Settings {
    /// Optional path to the zsh history file. When `None`, zsh history is not loaded.
    /// When `Some`, zsh history is loaded in addition to bash history; an empty string or no
    /// value means use the default path (`$HOME/.zsh_history`).
    pub zsh_history_path: Option<String>,
    /// Whether to show tutorial hints for first-time users.
    pub tutorial_mode: bool,
    /// Whether to disable all animations (cursor movement, cursor fading, dynamic time).
    pub disable_animations: bool,
    /// Whether to disable automatic closing character insertion.
    pub disable_auto_closing_char: bool,
    /// Whether to use the terminal emulator's cursor instead of rendering a custom cursor.
    pub use_term_emulator_cursor: bool,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Command (and arguments) to invoke for AI mode. The current buffer is appended as the
    /// final argument. Empty means AI mode is not configured.
    pub ai_command: Vec<String>,
    /// Optional system prompt prepended to the buffer when invoking AI mode.
    /// When set, the subprocess receives `"<system_prompt>\n<buffer>"` as its final argument.
    pub ai_system_prompt: Option<String>,
    /// Custom prompt animations registered with `flyline create-anim`.
    pub custom_animations: HashMap<String, PromptAnimation>,
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}
