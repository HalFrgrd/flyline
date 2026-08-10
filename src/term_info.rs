use crate::bash_funcs;
use std::sync::LazyLock;

/// Represents known terminal emulators and their capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEmulator {
    Kitty,
    Ghostty,
    VSCode,
    WezTerm,
    Foot,
    Rio,
    Alacritty,
    ITerm2,
    Xterm,
    Unknown(String),
}

impl TerminalEmulator {
    /// Returns a human-readable display name for the terminal emulator.
    pub fn name(&self) -> &str {
        match self {
            Self::Kitty => "Kitty",
            Self::Ghostty => "Ghostty",
            Self::VSCode => "VSCode Terminal",
            Self::WezTerm => "WezTerm",
            Self::Foot => "Foot",
            Self::Rio => "Rio",
            Self::Alacritty => "Alacritty",
            Self::ITerm2 => "iTerm2",
            Self::Xterm => "xterm",
            Self::Unknown(name) => name.as_str(),
        }
    }

    /// Detect whether this terminal emulator is known to support the Kitty extended keyboard protocol.
    pub fn supports_kitty_keyboard(&self) -> bool {
        matches!(
            self,
            Self::Kitty | Self::Ghostty | Self::VSCode | Self::WezTerm | Self::Foot | Self::Rio
        )
    }
}

/// Represents known terminal multiplexers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Multiplexer {
    Tmux,
    Screen,
    Zellij,
    Byobu,
    Unknown(String),
}

impl Multiplexer {
    /// Returns a human-readable display name for the terminal multiplexer.
    pub fn name(&self) -> &str {
        match self {
            Self::Tmux => "tmux",
            Self::Screen => "GNU Screen",
            Self::Zellij => "Zellij",
            Self::Byobu => "Byobu",
            Self::Unknown(name) => name.as_str(),
        }
    }
}

/// Retrieve `$TERM` environment variable.
pub fn term() -> Option<String> {
    bash_funcs::get_envvar_value("TERM")
}

/// Retrieve `$TERM_PROGRAM` environment variable.
pub fn term_program() -> Option<String> {
    bash_funcs::get_envvar_value("TERM_PROGRAM")
}

/// Retrieve `$TERM_PROGRAM_VERSION` environment variable.
pub fn term_program_version() -> Option<String> {
    bash_funcs::get_envvar_value("TERM_PROGRAM_VERSION")
}

/// Detect the active terminal emulator from `TERM` and `TERM_PROGRAM` values.
pub fn detect_terminal_emulator_from_env(
    term: Option<&str>,
    term_program: Option<&str>,
) -> TerminalEmulator {
    let term_lower = term.unwrap_or_default().to_lowercase();
    let program_lower = term_program.unwrap_or_default().to_lowercase();

    if program_lower.contains("ghostty") {
        TerminalEmulator::Ghostty
    } else if program_lower.contains("vscode") {
        TerminalEmulator::VSCode
    } else if program_lower.contains("kitty") || term_lower.contains("xterm-kitty") {
        TerminalEmulator::Kitty
    } else if program_lower.contains("wezterm") {
        TerminalEmulator::WezTerm
    } else if program_lower.contains("foot") || term_lower.contains("foot") {
        TerminalEmulator::Foot
    } else if program_lower.contains("rio") {
        TerminalEmulator::Rio
    } else if program_lower.contains("alacritty") || term_lower.contains("alacritty") {
        TerminalEmulator::Alacritty
    } else if program_lower.contains("iterm") {
        TerminalEmulator::ITerm2
    } else if let Some(prog) = term_program.filter(|p| !p.is_empty()) {
        TerminalEmulator::Unknown(prog.to_string())
    } else if term_lower.contains("xterm") {
        TerminalEmulator::Xterm
    } else if let Some(t) = term.filter(|t| !t.is_empty()) {
        TerminalEmulator::Unknown(t.to_string())
    } else {
        TerminalEmulator::Unknown("unknown".to_string())
    }
}

/// Detect active multiplexer from environment variables.
pub fn detect_multiplexer_from_env(
    tmux: Option<&str>,
    sty: Option<&str>,
    zellij: Option<&str>,
    byobu: Option<&str>,
    term_program: Option<&str>,
    term: Option<&str>,
) -> Option<Multiplexer> {
    if byobu.is_some_and(|s| !s.is_empty()) {
        Some(Multiplexer::Byobu)
    } else if zellij.is_some_and(|s| !s.is_empty())
        || term_program.is_some_and(|p| p.to_lowercase().contains("zellij"))
    {
        Some(Multiplexer::Zellij)
    } else if tmux.is_some_and(|s| !s.is_empty())
        || term_program.is_some_and(|p| p.to_lowercase().contains("tmux"))
    {
        Some(Multiplexer::Tmux)
    } else if sty.is_some_and(|s| !s.is_empty())
        || term.is_some_and(|t| t.to_lowercase().starts_with("screen"))
    {
        Some(Multiplexer::Screen)
    } else {
        None
    }
}

static CURRENT_EMULATOR: LazyLock<TerminalEmulator> = LazyLock::new(|| {
    let t = term();
    let tp = term_program();
    detect_terminal_emulator_from_env(t.as_deref(), tp.as_deref())
});

static CURRENT_MULTIPLEXER: LazyLock<Option<Multiplexer>> = LazyLock::new(|| {
    let tmux = bash_funcs::get_envvar_value("TMUX");
    let sty = bash_funcs::get_envvar_value("STY");
    let zellij = bash_funcs::get_envvar_value("ZELLIJ")
        .or_else(|| bash_funcs::get_envvar_value("ZELLIJ_SESSION_NAME"));
    let byobu = bash_funcs::get_envvar_value("BYOBU_PREFIX")
        .or_else(|| bash_funcs::get_envvar_value("BYOBU_CONFIG_DIR"));
    let tp = term_program();
    let t = term();
    detect_multiplexer_from_env(
        tmux.as_deref(),
        sty.as_deref(),
        zellij.as_deref(),
        byobu.as_deref(),
        tp.as_deref(),
        t.as_deref(),
    )
});

/// Returns the detected active terminal emulator for the current process.
pub fn current() -> TerminalEmulator {
    CURRENT_EMULATOR.clone()
}

/// Returns the detected active terminal multiplexer for the current process, if any.
pub fn multiplexer() -> Option<Multiplexer> {
    CURRENT_MULTIPLEXER.clone()
}

/// Helper function checking if a terminal multiplexer is active.
pub fn is_multiplexer_active() -> bool {
    CURRENT_MULTIPLEXER.is_some()
}

/// Helper function checking if the current terminal is Kitty.
pub fn is_kitty() -> bool {
    matches!(current(), TerminalEmulator::Kitty)
}

/// Helper function checking if the current terminal is Ghostty.
pub fn is_ghostty() -> bool {
    matches!(current(), TerminalEmulator::Ghostty)
}

/// Helper function checking if the current terminal is VSCode's integrated terminal.
pub fn is_vscode() -> bool {
    matches!(current(), TerminalEmulator::VSCode)
}

/// Helper function checking if the current terminal supports Kitty keyboard protocol.
pub fn detect_kitty_keyboard_support() -> bool {
    current().supports_kitty_keyboard()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_terminal_emulator() {
        assert_eq!(
            detect_terminal_emulator_from_env(Some("xterm-kitty"), None),
            TerminalEmulator::Kitty
        );
        assert_eq!(
            detect_terminal_emulator_from_env(None, Some("ghostty")),
            TerminalEmulator::Ghostty
        );
        assert_eq!(
            detect_terminal_emulator_from_env(None, Some("vscode")),
            TerminalEmulator::VSCode
        );
        assert_eq!(
            detect_terminal_emulator_from_env(Some("xterm-256color"), Some("WezTerm")),
            TerminalEmulator::WezTerm
        );
        assert_eq!(
            detect_terminal_emulator_from_env(Some("foot"), None),
            TerminalEmulator::Foot
        );
        assert_eq!(
            detect_terminal_emulator_from_env(Some("xterm-256color"), Some("my-custom-term")),
            TerminalEmulator::Unknown("my-custom-term".to_string())
        );
    }

    #[test]
    fn test_detect_multiplexer() {
        assert_eq!(
            detect_multiplexer_from_env(
                Some("/tmp/tmux-1000/default,123,0"),
                None,
                None,
                None,
                None,
                None
            ),
            Some(Multiplexer::Tmux)
        );
        assert_eq!(
            detect_multiplexer_from_env(None, Some("1234.pts-0.host"), None, None, None, None),
            Some(Multiplexer::Screen)
        );
        assert_eq!(
            detect_multiplexer_from_env(None, None, Some("my_session"), None, None, None),
            Some(Multiplexer::Zellij)
        );
        assert_eq!(
            detect_multiplexer_from_env(None, None, None, Some("/usr"), None, None),
            Some(Multiplexer::Byobu)
        );
        assert_eq!(
            detect_multiplexer_from_env(None, None, None, None, None, Some("xterm-256color")),
            None
        );
    }

    #[test]
    fn test_kitty_keyboard_support() {
        assert!(TerminalEmulator::Kitty.supports_kitty_keyboard());
        assert!(TerminalEmulator::Ghostty.supports_kitty_keyboard());
        assert!(TerminalEmulator::VSCode.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Xterm.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Unknown("unknown".to_string()).supports_kitty_keyboard());
    }
}
