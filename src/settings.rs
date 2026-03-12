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
    /// Whether to use the terminal emulator's cursor instead of rendering a custom cursor.
    pub use_term_emulator_cursor: bool,
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}
