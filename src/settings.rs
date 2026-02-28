#[derive(Debug, Default)]
pub struct Settings {
    /// Whether to load zsh history in addition to bash history.
    pub load_zsh_history: bool,
    /// Whether to show tutorial hints for first-time users.
    pub tutorial_mode: bool,
    /// Chrono format string for FLYLINE_TIME (e.g. "%H:%M:%S"). None uses the default format.
    pub time_format: Option<String>,
}
