//! Terminal emulator and multiplexer detection.
//!
//! Device attribute parsing rules in this module are adapted from `ble.sh`
//! by Koichi Murase (@akinomyoga):
//! <https://github.com/akinomyoga/ble.sh/blob/master/src/util.sh>

use crate::bash_funcs;
use crate::settings::ResizeLogic;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// Represents known terminal emulators and their capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEmulator {
    Kitty,
    Ghostty,
    VSCode,
    Hyper,
    Tabby,
    WezTerm,
    Foot,
    Rio,
    Alacritty,
    ITerm2,
    Xterm,
    Konsole,
    VTE,
    Mintty,
    Terminology,
    WindowsTerminal,
    Mlterm,
    RLogin,
    Cygwin,
    Contra,
    Unknown(String),
}

impl TerminalEmulator {
    /// Returns a human-readable display name for the terminal emulator.
    pub fn name(&self) -> &str {
        match self {
            Self::Kitty => "Kitty",
            Self::Ghostty => "Ghostty",
            Self::VSCode => "VSCode Terminal",
            Self::Hyper => "Hyper",
            Self::Tabby => "Tabby",
            Self::WezTerm => "WezTerm",
            Self::Foot => "Foot",
            Self::Rio => "Rio",
            Self::Alacritty => "Alacritty",
            Self::ITerm2 => "iTerm2",
            Self::Xterm => "xterm",
            Self::Konsole => "Konsole",
            Self::VTE => "VTE",
            Self::Mintty => "mintty",
            Self::Terminology => "Terminology",
            Self::WindowsTerminal => "Windows Terminal",
            Self::Mlterm => "mlterm",
            Self::RLogin => "RLogin",
            Self::Cygwin => "Cygwin Terminal",
            Self::Contra => "Contra",
            Self::Unknown(name) => name.as_str(),
        }
    }

    /// Detect whether this terminal emulator is known to support the Kitty extended keyboard protocol.
    pub fn supports_kitty_keyboard(&self) -> bool {
        matches!(
            self,
            Self::Kitty
                | Self::Ghostty
                | Self::VSCode
                | Self::Hyper
                | Self::Tabby
                | Self::WezTerm
                | Self::Foot
                | Self::Rio
        )
    }

    /// Returns true if the terminal emulator is built on xterm.js.
    pub fn is_xterm_js_based(&self) -> bool {
        matches!(self, Self::VSCode | Self::Hyper | Self::Tabby)
            || self.name().to_lowercase().contains("xterm")
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

/// Represents device attribute detection results (DA1 or DA2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAttributes {
    /// The raw response string returned by the terminal (e.g. ">0;271;0" or "1;277;0").
    pub raw: String,
    /// Terminal emulator identified from DA response, if any.
    pub emulator: Option<TerminalEmulator>,
    /// Terminal multiplexer identified from DA response, if any.
    pub multiplexer: Option<Multiplexer>,
    /// Version string extracted from DA response, if any.
    pub version: Option<String>,
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
    } else if program_lower.contains("hyper") {
        TerminalEmulator::Hyper
    } else if program_lower.contains("tabby") || program_lower.contains("terminus") {
        TerminalEmulator::Tabby
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

/// Parse device attribute response (DA1 or DA2) according to `ble.sh`'s detection rules.
///
/// Identification rules are adapted from `ble/term/DA2/initialize-term` in `ble.sh`:
/// <https://github.com/akinomyoga/ble.sh/blob/master/src/util.sh>
pub fn parse_da_response(raw: &str) -> DeviceAttributes {
    let raw_trimmed = raw.trim();
    let clean = raw_trimmed
        .strip_prefix('>')
        .or_else(|| raw_trimmed.strip_prefix('?'))
        .unwrap_or(raw_trimmed);

    let parts: Vec<&str> = clean.split(';').collect();
    let da2r_vec: Vec<u64> = parts
        .iter()
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect();

    let p0 = da2r_vec.get(0).copied().unwrap_or(0);
    let p1 = da2r_vec.get(1).copied().unwrap_or(0);
    let p2 = da2r_vec.get(2).copied().unwrap_or(0);

    let mut emulator = None;
    let mut multiplexer = None;
    let mut version = None;

    match clean {
        "0;271;0" => {
            emulator = Some(TerminalEmulator::Terminology);
            version = Some("200".to_string());
        }
        "41;285;0" => {
            emulator = Some(TerminalEmulator::Terminology);
            version = Some("300".to_string());
        }
        "61;337;0" => {
            emulator = Some(TerminalEmulator::Terminology);
            version = Some("10400".to_string());
        }
        "0;0;0" => {
            emulator = Some(TerminalEmulator::WezTerm);
            version = Some("0".to_string());
        }
        "1;277;0" => {
            emulator = Some(TerminalEmulator::WezTerm);
            version = Some("20220408".to_string());
        }
        "0;115;0" => {
            emulator = Some(TerminalEmulator::Konsole);
            version = Some("30000".to_string());
        }
        "1;115;0" => {
            emulator = Some(TerminalEmulator::Konsole);
            version = Some("220380".to_string());
        }
        "1;96;0" => {
            emulator = Some(TerminalEmulator::Mlterm);
            version = Some("30102".to_string());
        }
        "24;279;0" => {
            emulator = Some(TerminalEmulator::Mlterm);
            version = Some("30702".to_string());
        }
        "0;95;0" => {
            emulator = Some(TerminalEmulator::ITerm2);
            let ver = bash_funcs::get_envvar_value("LC_TERMINAL_VERSION")
                .unwrap_or_else(|| "2.9+".to_string());
            version = Some(ver);
        }
        "41;2500;0" => {
            emulator = Some(TerminalEmulator::ITerm2);
            let ver = bash_funcs::get_envvar_value("LC_TERMINAL_VERSION")
                .unwrap_or_else(|| "3.5.0+".to_string());
            version = Some(ver);
        }
        "64;2500;0" => {
            emulator = Some(TerminalEmulator::ITerm2);
            let ver = bash_funcs::get_envvar_value("LC_TERMINAL_VERSION")
                .unwrap_or_else(|| "3.5.6+".to_string());
            version = Some(ver);
        }
        "0;10;1" => {
            emulator = Some(TerminalEmulator::WindowsTerminal);
            version = Some("0".to_string());
        }
        "1;10;0" => {
            emulator = Some(TerminalEmulator::Ghostty);
            version = Some("10000".to_string());
        }
        "84;0;0" => {
            multiplexer = Some(Multiplexer::Tmux);
            version = Some("0".to_string());
        }
        _ => {
            if p0 == 0 && p2 == 1 {
                if p1 >= 3000 {
                    multiplexer = Some(Multiplexer::Zellij);
                    version = Some(p1.to_string());
                } else if p1 >= 1001 {
                    emulator = Some(TerminalEmulator::Alacritty);
                    version = Some(p1.to_string());
                }
            } else if p0 == 1
                && p2 == 0
                && parts
                    .get(1)
                    .map_or(false, |s| s.starts_with('0') && s.len() == 6)
            {
                emulator = Some(TerminalEmulator::Foot);
                let p1_str = parts[1];
                let ver_str = if p1_str.len() > 1 {
                    &p1_str[1..]
                } else {
                    p1_str
                };
                version = Some(ver_str.to_string());
            } else if p0 == 1 {
                if p1 >= 4000 && p1 <= 4009 && p2 >= 3 {
                    emulator = Some(TerminalEmulator::Kitty);
                    version = Some((p1 - 4000).to_string());
                } else if p1 >= 803 && p1 < 5400 && p2 == 0 {
                    emulator = Some(TerminalEmulator::VTE);
                    version = Some(p1.to_string());
                }
            } else if p0 == 61 {
                if p1 >= 7501 && p2 == 1 {
                    emulator = Some(TerminalEmulator::VTE);
                    version = Some(p1.to_string());
                }
            } else if p0 == 65 {
                if p1 >= 5300 && p1 <= 7501 && p2 == 1 {
                    emulator = Some(TerminalEmulator::VTE);
                    version = Some(p1.to_string());
                } else if p1 >= 100 {
                    emulator = Some(TerminalEmulator::RLogin);
                    version = Some(p1.to_string());
                }
            } else if p0 == 67 {
                if p2 == 0 && p1 >= 100 {
                    emulator = Some(TerminalEmulator::Cygwin);
                    version = Some(p1.to_string());
                }
            } else if p0 == 77 && p2 == 0 {
                emulator = Some(TerminalEmulator::Mintty);
                version = Some(p1.to_string());
            } else if p0 == 83 && p2 == 0 {
                multiplexer = Some(Multiplexer::Screen);
                version = Some(p1.to_string());
            } else if p0 == 99 {
                emulator = Some(TerminalEmulator::Contra);
                version = Some(p1.to_string());
            } else {
                let term_env = term().unwrap_or_default().to_lowercase();
                if term_env.starts_with("xterm") {
                    let matches_xterm = (p0 == 1 && p2 == 0)
                        || (p0 == 0 && p2 == 0 && p1 >= 95)
                        || (matches!(p0, 2 | 24 | 18 | 19 | 41 | 61 | 64 | 65)
                            && p2 == 0
                            && p1 >= 280)
                        || (p0 == 32 && p2 == 0 && p1 >= 354 && p1 < 2000);

                    if matches_xterm {
                        emulator = Some(TerminalEmulator::Xterm);
                        version = Some(p1.to_string());
                    }
                }
            }
        }
    }

    if emulator.is_none() && multiplexer.is_none() {
        emulator = Some(TerminalEmulator::Unknown(raw_trimmed.to_string()));
    }

    DeviceAttributes {
        raw: raw_trimmed.to_string(),
        emulator,
        multiplexer,
        version,
    }
}

/// Stores information about the active terminal emulator, multiplexer, and device attributes.
///
/// Device attribute detection rules are ported from [`ble.sh`](https://github.com/akinomyoga/ble.sh) (`src/util.sh`):
/// <https://github.com/akinomyoga/ble.sh/blob/master/src/util.sh>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermInfo {
    pub emulator: TerminalEmulator,
    pub multiplexer: Option<Multiplexer>,
    pub device_attributes: Option<DeviceAttributes>,
}

impl TermInfo {
    /// Create a new `TermInfo` by querying the active terminal via an `EventReader`.
    ///
    /// Sends DA2 request (`\x1b[>c`) and blocks for up to 300ms to parse device attributes.
    /// Falls back to environment variable detection for any unresolved fields.
    pub fn new(reader: &termina::EventReader) -> Self {
        Self::new_with_timeout(reader, Some(Duration::from_millis(300)))
    }

    /// Create a new `TermInfo` querying the terminal with a specified timeout.
    pub fn new_with_timeout(reader: &termina::EventReader, timeout: Option<Duration>) -> Self {
        let da = match query_da_from_reader(reader, timeout) {
            Ok(da) => da,
            Err(e) => {
                log::warn!("Failed to query device attributes: {}", e);
                None
            }
        };

        let env_emulator =
            detect_terminal_emulator_from_env(term().as_deref(), term_program().as_deref());
        let env_multiplexer = detect_multiplexer_from_env(
            bash_funcs::get_envvar_value("TMUX").as_deref(),
            bash_funcs::get_envvar_value("STY").as_deref(),
            bash_funcs::get_envvar_value("ZELLIJ")
                .or_else(|| bash_funcs::get_envvar_value("ZELLIJ_SESSION_NAME"))
                .as_deref(),
            bash_funcs::get_envvar_value("BYOBU_PREFIX")
                .or_else(|| bash_funcs::get_envvar_value("BYOBU_CONFIG_DIR"))
                .as_deref(),
            term_program().as_deref(),
            term().as_deref(),
        );

        let (emulator, multiplexer) = if let Some(ref da_res) = da {
            let em = match &da_res.emulator {
                Some(e) if !matches!(e, TerminalEmulator::Unknown(_)) => e.clone(),
                _ if !matches!(env_emulator, TerminalEmulator::Unknown(_)) => env_emulator,
                Some(e) => e.clone(),
                None => env_emulator,
            };
            let mult = da_res.multiplexer.clone().or(env_multiplexer);
            (em, mult)
        } else {
            (env_emulator, env_multiplexer)
        };

        Self {
            emulator,
            multiplexer,
            device_attributes: da,
        }
    }

    /// Construct `TermInfo` purely from environment variable detection (non-blocking).
    pub fn from_env() -> Self {
        let emulator =
            detect_terminal_emulator_from_env(term().as_deref(), term_program().as_deref());
        let multiplexer = detect_multiplexer_from_env(
            bash_funcs::get_envvar_value("TMUX").as_deref(),
            bash_funcs::get_envvar_value("STY").as_deref(),
            bash_funcs::get_envvar_value("ZELLIJ")
                .or_else(|| bash_funcs::get_envvar_value("ZELLIJ_SESSION_NAME"))
                .as_deref(),
            bash_funcs::get_envvar_value("BYOBU_PREFIX")
                .or_else(|| bash_funcs::get_envvar_value("BYOBU_CONFIG_DIR"))
                .as_deref(),
            term_program().as_deref(),
            term().as_deref(),
        );
        Self {
            emulator,
            multiplexer,
            device_attributes: None,
        }
    }
}

static STATIC_TERM_INFO: Mutex<Option<TermInfo>> = Mutex::new(None);

/// Access the static [`TermInfo`].
///
/// If already initialized, returns the static struct.
/// If not yet initialized, queries the terminal via `reader` (blocking),
/// initializes `TermInfo::new(reader)`, stores it in the static instance, and returns it.
pub fn get_term_info(reader: &termina::EventReader) -> TermInfo {
    if let Ok(guard) = STATIC_TERM_INFO.lock() {
        if let Some(ref info) = *guard {
            return info.clone();
        }
    }

    let info = TermInfo::new(reader);
    if let Ok(mut guard) = STATIC_TERM_INFO.lock() {
        if guard.is_none() {
            *guard = Some(info.clone());
        } else if let Some(ref existing) = *guard {
            return existing.clone();
        }
    }

    info
}

/// Returns the cached [`DeviceAttributes`] if device attribute querying has been performed.
pub fn device_attributes() -> Option<DeviceAttributes> {
    get_term_info(&crate::app::GLOBAL_EVENT_READER).device_attributes
}

/// Returns the detected active terminal emulator for the current process.
pub fn current() -> TerminalEmulator {
    get_term_info(&crate::app::GLOBAL_EVENT_READER).emulator
}

/// Returns the detected active terminal multiplexer for the current process, if any.
pub fn multiplexer() -> Option<Multiplexer> {
    get_term_info(&crate::app::GLOBAL_EVENT_READER).multiplexer
}

/// Helper function to perform DA query on an EventReader.
fn query_da_from_reader(
    reader: &termina::EventReader,
    timeout: Option<Duration>,
) -> std::io::Result<Option<DeviceAttributes>> {
    use std::io::Write;
    use termina::Event as TerminaEvent;
    use termina::escape::csi::{Csi, Device as CsiDevice};

    let req_csi = Csi::Device(CsiDevice::RequestSecondaryDeviceAttributes);
    let mut stdout = std::io::stdout();
    write!(stdout, "{req_csi}")?;
    stdout.flush()?;

    let timeout = timeout.unwrap_or(Duration::from_millis(300));

    let is_da_event = |event: &TerminaEvent| {
        matches!(
            event,
            TerminaEvent::Csi(Csi::Device(CsiDevice::DeviceAttributes(_)))
        )
    };

    if reader.poll(Some(timeout), is_da_event)? {
        if let TerminaEvent::Csi(Csi::Device(CsiDevice::DeviceAttributes(raw))) =
            reader.read(is_da_event)?
        {
            let da = parse_da_response(&raw);
            return Ok(Some(da));
        }
    }

    Ok(None)
}

/// Helper function checking if a terminal multiplexer is active.
pub fn is_multiplexer_active() -> bool {
    multiplexer().is_some()
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

/// Determines the recommended default resize strategy based on terminal emulator detection.
pub fn default_resize_logic() -> ResizeLogic {
    let em = current();
    let mult = multiplexer();
    if mult == Some(Multiplexer::Zellij) {
        ResizeLogic::ReflowedAllWhitespaceTrimmed
    } else if mult.is_some() {
        ResizeLogic::ReflowedAll
    } else if matches!(em, TerminalEmulator::Ghostty | TerminalEmulator::Kitty) {
        ResizeLogic::AutoCleared
    } else if em.is_xterm_js_based() {
        ResizeLogic::ReflowedApartFromCursor
    } else {
        ResizeLogic::ReflowedAll
    }
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
            detect_terminal_emulator_from_env(None, Some("Hyper")),
            TerminalEmulator::Hyper
        );
        assert_eq!(
            detect_terminal_emulator_from_env(None, Some("Tabby")),
            TerminalEmulator::Tabby
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
    fn test_parse_da_responses_ble_sh() {
        // Terminology
        let da = parse_da_response(">0;271;0");
        assert_eq!(da.emulator, Some(TerminalEmulator::Terminology));
        assert_eq!(da.version.as_deref(), Some("200"));

        // WezTerm
        let da = parse_da_response(">0;0;0");
        assert_eq!(da.emulator, Some(TerminalEmulator::WezTerm));
        assert_eq!(da.version.as_deref(), Some("0"));

        let da = parse_da_response(">1;277;0");
        assert_eq!(da.emulator, Some(TerminalEmulator::WezTerm));
        assert_eq!(da.version.as_deref(), Some("20220408"));

        // Konsole
        let da = parse_da_response(">0;115;0");
        assert_eq!(da.emulator, Some(TerminalEmulator::Konsole));
        assert_eq!(da.version.as_deref(), Some("30000"));

        // Mlterm
        let da = parse_da_response(">1;96;0");
        assert_eq!(da.emulator, Some(TerminalEmulator::Mlterm));
        assert_eq!(da.version.as_deref(), Some("30102"));

        // Windows Terminal
        let da = parse_da_response(">0;10;1");
        assert_eq!(da.emulator, Some(TerminalEmulator::WindowsTerminal));

        // Zellij vs Alacritty
        let da_zellij = parse_da_response(">0;3000;1");
        assert_eq!(da_zellij.multiplexer, Some(Multiplexer::Zellij));

        let da_alacritty = parse_da_response(">0;1001;1");
        assert_eq!(da_alacritty.emulator, Some(TerminalEmulator::Alacritty));
        assert_eq!(da_alacritty.version.as_deref(), Some("1001"));

        // Ghostty
        let da_ghostty = parse_da_response(">1;10;0");
        assert_eq!(da_ghostty.emulator, Some(TerminalEmulator::Ghostty));
        assert_eq!(da_ghostty.version.as_deref(), Some("10000"));

        // Foot
        let da_foot = parse_da_response(">1;000500;0");
        assert_eq!(da_foot.emulator, Some(TerminalEmulator::Foot));
        assert_eq!(da_foot.version.as_deref(), Some("00500"));

        // Kitty
        let da_kitty = parse_da_response(">1;4000;3");
        assert_eq!(da_kitty.emulator, Some(TerminalEmulator::Kitty));
        assert_eq!(da_kitty.version.as_deref(), Some("0"));

        // VTE
        let da_vte = parse_da_response(">1;5400;0");
        assert_eq!(
            da_vte.emulator,
            Some(TerminalEmulator::Unknown(">1;5400;0".to_string()))
        );

        let da_vte2 = parse_da_response(">61;7501;1");
        assert_eq!(da_vte2.emulator, Some(TerminalEmulator::VTE));
        assert_eq!(da_vte2.version.as_deref(), Some("7501"));

        // Mintty
        let da_mintty = parse_da_response(">77;302;0");
        assert_eq!(da_mintty.emulator, Some(TerminalEmulator::Mintty));
        assert_eq!(da_mintty.version.as_deref(), Some("302"));

        // Tmux & Screen
        let da_tmux = parse_da_response(">84;0;0");
        assert_eq!(da_tmux.multiplexer, Some(Multiplexer::Tmux));

        let da_screen = parse_da_response(">83;4000;0");
        assert_eq!(da_screen.multiplexer, Some(Multiplexer::Screen));
    }

    #[test]
    fn test_kitty_keyboard_support() {
        assert!(TerminalEmulator::Kitty.supports_kitty_keyboard());
        assert!(TerminalEmulator::Ghostty.supports_kitty_keyboard());
        assert!(TerminalEmulator::VSCode.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Xterm.supports_kitty_keyboard());
        assert!(!TerminalEmulator::Unknown("unknown".to_string()).supports_kitty_keyboard());
    }

    #[test]
    fn test_default_resize_logic() {
        use crate::settings::ResizeLogic;
        let ghostty = detect_terminal_emulator_from_env(None, Some("ghostty"));
        assert_eq!(
            if true {
                ResizeLogic::ReflowedAll
            } else if matches!(ghostty, TerminalEmulator::Ghostty | TerminalEmulator::Kitty) {
                ResizeLogic::AutoCleared
            } else {
                ResizeLogic::ReflowedAll
            },
            ResizeLogic::ReflowedAll
        );

        assert_eq!(
            if false {
                ResizeLogic::ReflowedAll
            } else if matches!(ghostty, TerminalEmulator::Ghostty | TerminalEmulator::Kitty) {
                ResizeLogic::AutoCleared
            } else {
                ResizeLogic::ReflowedAll
            },
            ResizeLogic::AutoCleared
        );
    }
}
