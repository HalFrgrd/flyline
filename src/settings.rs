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
    /// Whether to load zsh history in addition to bash history.
    pub load_zsh_history: bool,
    /// Whether to show tutorial hints for first-time users.
    pub tutorial_mode: bool,
    /// Whether to disable all animations (cursor movement, cursor fading, dynamic time).
    pub disable_animations: bool,
    /// Whether to disable automatic closing character insertion.
    pub disable_auto_closing_char: bool,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Command (and arguments) to invoke for AI mode. The current buffer is appended as the
    /// final argument. Empty means AI mode is not configured.
    pub ai_command: Vec<String>,
    /// Custom prompt animations registered with `flyline create-anim`.
    pub custom_animations: Vec<PromptAnimation>,
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}
