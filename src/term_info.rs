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

static CURRENT_EMULATOR: LazyLock<TerminalEmulator> = LazyLock::new(|| {
    let t = term();
    let tp = term_program();
    detect_terminal_emulator_from_env(t.as_deref(), tp.as_deref())
});

/// Returns the detected active terminal emulator for the current process.
pub fn current() -> TerminalEmulator {
    CURRENT_EMULATOR.clone()
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
    fn test_kitty_keyboard_support() {
        assert!(TerminalEmulator::Kitty.supports_kitty_keyboard());
        assert!(TerminalEmulator::Ghostty.supports_kitty_keyboard());
        assert!(TerminalEmulator::VSCode.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Xterm.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Unknown("unknown".to_string()).supports_kitty_keyboard());
    }
}
