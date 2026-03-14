/// Controls how flyline manages mouse capture.
#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq, Default)]
pub enum MouseMode {
    /// Never capture mouse events.
    Disabled,
    /// Mouse capture is on by default; toggled when Escape is pressed or Alt is pressed/released.
    Simple,
    /// Mouse capture is on by default with automatic management: disabled on scroll or when the
    /// mouse moves above the viewport, re-enabled on any keypress and every 500 ms.
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
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}
