use std::collections::HashMap;
use std::path::PathBuf;

use crate::app::actions;
use crate::content::TaggedSpan;
use crate::cursor::CursorConfig;
use crate::palette::Palette;
use crate::term_info;
use crate::tutorial::TutorialStep;
use clap::ValueEnum;

pub use flycontent::palette::ColourTheme;

/// Configures which history storage backend is active.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum HistoryBackend {
    /// Use Flyline's JSONL history file (~/.local/share/flyline/history.jsonl).
    #[value(name = "flyline")]
    #[serde(rename = "flyline")]
    Flyline,
    /// Use standard GNU Bash in-memory history.
    #[default]
    #[value(name = "bash")]
    #[serde(rename = "bash")]
    Bash,
}

/// How suggestions should be sorted when fuzzy scores are tied.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum SuggestionSortOrder {
    /// Sort by last modification time (if available), then alphabetically.
    #[default]
    Mtime,
    /// Sort alphabetically.
    Alphabetical,
}

/// Controls fuzzy matching behavior for suggestions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum FuzzyMode {
    /// Enable fuzzy matching for all completions.
    #[default]
    #[value(name = "all")]
    #[serde(rename = "all")]
    All,
    /// Disable fuzzy matching (use prefix matching instead).
    #[value(name = "none")]
    #[serde(rename = "none")]
    None,
    /// Match folders using prefix matching instead of fuzzy matching.
    #[value(name = "folder-prefixes")]
    #[serde(rename = "folder-prefixes")]
    FolderPrefixes,
}

/// A single custom prompt animation registered with `flyline create-prompt-widget animation`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// A custom prompt widget registered with `flyline create-prompt-widget`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PromptWidget {
    /// Show different text depending on whether mouse capture is enabled.
    MouseMode {
        /// Name used as placeholder in prompt strings (e.g., `FLYLINE_MOUSE_MODE`).
        name: String,
        /// Text shown when mouse capture is enabled.
        enabled_text: String,
        /// Text shown when mouse capture is disabled.
        disabled_text: String,
    },
    /// Copies the current command buffer to the clipboard when clicked.
    CopyBuffer {
        /// Name used as placeholder in prompt strings (e.g., `FLYLINE_COPY_BUFFER`).
        name: String,
        /// Text shown in the prompt.
        text: String,
    },
    /// Runs a shell command and displays its output. Kept as a named struct
    /// because methods/helpers (e.g. `resolve_placeholder`) take `&PromptWidgetCustom`
    /// directly.
    Custom(PromptWidgetCustom),
    /// Shows how long ago the flyline app last closed.
    ///
    /// The elapsed duration is formatted as a compact human-readable string,
    /// for example `9.2s`, `1m23s`, `1h02m03s`, `1d20h43m`.
    LastCommandDuration {
        /// Name used as placeholder in prompt strings (e.g., `FLYLINE_LAST_COMMAND_DURATION`).
        name: String,
    },
    /// Show different text depending on whether the leader key is active.
    LeaderMode {
        /// Name used as placeholder in prompt strings (e.g., `FLYLINE_LEADER_MODE`).
        name: String,
        /// Text shown when the leader key is active.
        active_text: String,
        /// Text shown when the leader key is inactive.
        inactive_text: String,
    },
    /// Widget that displays the line number for multi-line continuation prompt.
    BufferLineNumber {
        /// Name used as placeholder in prompt strings (e.g., `FLYLINE_PROMPT_LINE_NUMBER`).
        name: String,
    },
}

impl PromptWidget {
    /// The placeholder name that is replaced inside prompt strings (PS1, RPS1, PS1_FILL, PS2, PROMPT_RULER).
    pub fn name(&self) -> &str {
        match self {
            PromptWidget::MouseMode { name, .. } => name,
            PromptWidget::CopyBuffer { name, .. } => name,
            PromptWidget::Custom(w) => &w.name,
            PromptWidget::LastCommandDuration { name } => name,
            PromptWidget::LeaderMode { name, .. } => name,
            PromptWidget::BufferLineNumber { name } => name,
        }
    }
}

/// What to show as a placeholder while a non-blocking (or timed-out blocking)
/// custom widget command is still running.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum Placeholder {
    /// Show N spaces.
    Spaces(usize),
    /// Show the previous output of the command (empty on the very first run).
    #[default]
    Prev,
}

/// A prompt widget that runs a shell command and displays its output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromptWidgetCustom {
    /// Name used as placeholder in prompt strings (e.g., `CUSTOM_WIDGET1`).
    pub name: String,
    /// Command (and arguments) to run.
    pub command: Vec<String>,
    /// Timeout in milliseconds to wait for the command before rendering the
    /// first prompt frame.  `None` (not specified) defaults to `0`, meaning a
    /// single non-blocking `try_wait` is performed at spawn time — the command
    /// immediately goes to the background if it hasn't finished.  `Some(n)`
    /// polls for up to `n` milliseconds; `Some(i32::MAX)` (~24.8 days) is
    /// effectively indefinite.
    pub block: Option<i32>,
    /// What to show while the command is running (or has timed out).
    pub placeholder: Placeholder,
    /// Most recent successful output of the command; shared across clones so
    /// that the `Placeholder::Prev` option can pick it up on subsequent renders.
    #[serde(skip)]
    pub prev_output: std::sync::Arc<std::sync::Mutex<Vec<TaggedSpan<'static>>>>,
}

/// A configured agent-mode command with its optional system prompt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentModeCommand {
    /// Command (and arguments) to invoke. The current buffer is appended as the
    /// final argument.  Stored as a `Vec<String>` after splitting the
    /// user-supplied command string on whitespace.
    pub command: Vec<String>,
    /// Optional system prompt prepended to the buffer when invoking AI mode.
    /// When set, the subprocess receives `"<system_prompt>\n<buffer>"` as its final argument.
    pub system_prompt: Option<String>,
}

/// Controls whether and when the matrix animation is shown.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MatrixAnimation {
    /// Never show the matrix animation.
    #[default]
    Off,
    /// Always show the matrix animation.
    On,
    /// Show the matrix animation only after the given number of seconds of inactivity
    /// (no keypress or mouse event).
    IdleSecs(u64),
}

/// Controls how flyline manages mouse capture.
#[derive(
    clap::ValueEnum,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum MouseMode {
    /// Never capture mouse events.
    Disabled,
    /// Mouse capture is on by default; toggled when Escape is pressed.
    Simple,
    /// Mouse capture is on by default with automatic management: disabled on scroll or when the
    /// user clicks above the viewport, re-enabled on any keypress or when focus is regained.
    /// Also can manually toggle with Escape.
    #[default]
    Smart,
}

/// How many shell integration escape codes (OSC 133 / OSC 633) flyline sends.
#[derive(
    clap::ValueEnum,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ShellIntegrationLevel {
    /// Send no shell integration codes.
    None,
    /// Only send the escape codes that report prompt start/end positions.
    #[default]
    OnlyPromptPos,
    /// Send the full set of shell integration codes: prompt positions, execution
    /// start/end codes, and cursor-position reporting.
    Full,
}

#[derive(
    clap::ValueEnum,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ResizeLogic {
    /// Automatically decide based on terminal emulator (default).
    #[default]
    Default,
    /// Do not move the cursor on window resize.
    AutoCleared,
    /// Move cursor up by H rows (where H is current inline cursor Y).
    ReflowedApartFromCursor,
    /// Move cursor up by H rows (where H is current inline cursor Y).
    ReflowedAll,
    /// Move cursor up accounting for line reflow, treating trailing whitespace as empty.
    ReflowedAllWhitespaceTrimmed,
    /// Do not perform any cursor adjustment on resize.
    DontMoveCursor,
}

impl ResizeLogic {
    /// Resolves `Default` to the automatic terminal-specific recommendation,
    /// or returns the explicit user-configured strategy.
    pub fn resolve(self) -> Self {
        if self == Self::Default {
            term_info::default_resize_logic()
        } else {
            self
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Settings {
    /// Optional path to the Zsh history file. When `None`, Zsh history is not loaded.
    /// When `Some`, Zsh history is loaded in addition to Bash history; an empty string or no
    /// value means use the default path (`$HOME/.zsh_history`).
    #[serde(rename = "history.zsh_path")]
    pub zsh_history_path: Option<String>,
    /// Whether the interactive tutorial is active.
    pub run_tutorial: bool,
    /// Current tutorial step.
    #[serde(skip)]
    pub tutorial_step: TutorialStep,
    /// Whether to show all animations (cursor movement, cursor fading, dynamic time).
    pub show_animations: bool,
    /// Whether to show inline history suggestions.
    pub show_inline_history: bool,
    /// Whether to auto-start tab completion suggestions as you type.
    pub auto_suggest: bool,
    /// Whether to show last modification timestamps for Git references (branches, tags, stashes).
    #[serde(rename = "suggestions.git_ref_mtime")]
    pub git_ref_mtime: bool,
    /// Settings for flycomp shell completion synthesis.
    #[serde(rename = "flycomp")]
    pub flycomp: flycomp::FlycompSettings,
    /// How to sort suggestions when fuzzy scores are tied.
    pub suggestion_sort_order: SuggestionSortOrder,
    /// Controls fuzzy matching behavior for suggestions.
    pub fuzzy_mode: FuzzyMode,
    /// Maximum number of suggestion rows to render for tab-completion lists.
    pub num_suggestion_rows: u16,
    /// Whether to automatically close opening characters (e.g., parentheses, brackets, quotes).
    pub auto_close_chars: bool,
    /// Whether mouse clicks and drags on the command buffer change the cursor
    /// position and selection. When `false`, mouse interaction with the buffer
    /// does not change the buffer selection or cursor position.
    pub select_with_mouse: bool,
    /// Cursor appearance and animation settings (set via `flyline set-cursor`).
    #[serde(rename = "cursor")]
    pub cursor_config: CursorConfig,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Agent-mode commands keyed by optional trigger prefix.
    /// - `None` key: the default command invoked via Alt+Enter (no prefix match needed).
    /// - `Some(prefix)` key: activated when the user presses Enter and the buffer starts
    ///   with `prefix`; the prefix is stripped before the buffer is sent to the command.
    #[serde(serialize_with = "serialize_agent_commands")]
    pub agent_commands: HashMap<Option<String>, AgentModeCommand>,
    /// Custom prompt animations registered with `flyline create-prompt-widget animation`.
    pub custom_animations: HashMap<String, PromptAnimation>,
    /// Custom prompt widgets registered with `flyline create-prompt-widget`.
    pub custom_prompt_widgets: HashMap<String, PromptWidget>,
    /// Run matrix animation in the terminal background.
    pub matrix_animation: MatrixAnimation,
    /// Render frame rate in frames per second (1–120).
    pub frame_rate: u8,
    /// Idle frame rate in frames per second when inactive (default 0.2).
    pub idle_frame_rate: f64,
    /// Shell integration escape codes level (OSC 133 / OSC 633).
    pub send_shell_integration_codes: ShellIntegrationLevel,
    /// Whether to request the use of extended (kitty-protocol) keyboard codes
    /// during startup. Enabling this gives flyline more accurate keyboard
    /// events on terminals that support the protocol; disable it if your
    /// terminal misbehaves when the request is sent. Enabled by default.
    pub enable_extended_key_codes: bool,
    /// Whether easter eggs (such as animated command words like `python`) are enabled.
    /// Enabled by default; pass `--enable-easter-eggs false` to disable.
    pub enable_easter_eggs: bool,
    /// Active colour theme preset (`dark` / `light`). Used for theme-aware
    /// cursor fading; updated when `flyline set-style --default-theme` is run.
    pub colour_theme: ColourTheme,
    /// Configurable colour palette for UI elements.
    #[serde(rename = "palette")]
    pub colour_palette: Palette,
    /// User defined keybindings
    #[serde(serialize_with = "serialize_keybindings", rename = "user_keybindings")]
    pub keybindings: Vec<actions::Binding>,
    /// User defined key remappings (applied before matching bindings).
    #[serde(serialize_with = "serialize_key_remappings")]
    pub key_remappings: Vec<actions::KeyRemap>,
    /// Whether built-in default keybindings should be ignored.
    pub clear_default_keybindings: bool,
    /// Show the last key event and dispatched action above the prompt.
    pub key_debug: bool,
    /// Show the last mouse event above the prompt.
    pub mouse_debug: bool,
    /// Whether to change the mouse cursor shape depending on what is hovered.
    pub mouse_change_shape: bool,
    /// Path to Flyline JSONL history file.
    #[serde(rename = "history.jsonl_path")]
    pub history_jsonl_path: PathBuf,
    /// Timestamp of the most recent flyline app session close.
    ///
    /// Set to `Some(Instant::now())` immediately after each `app::get_command`
    /// call returns. Used by the `last-command-duration` prompt widget to
    /// compute and display the elapsed time since the last command.
    #[serde(skip)]
    pub last_app_closed_at: Option<std::time::Instant>,
    /// Initial buffer content to pre-fill the command line when Flyline starts.
    #[serde(skip)]
    pub initial_buffer: Option<String>,
    /// Resize logic strategy for cursor placement on window resize.
    pub resize_logic: ResizeLogic,
    /// Delay in milliseconds before performing delayed startup initialization (such as CPR and focus tracking).
    pub delayed_startup_ms: u64,
    /// Configured history storage backend (flyline, bash, or atuin).
    #[serde(rename = "history.backend")]
    pub history_backend: HistoryBackend,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zsh_history_path: None,
            run_tutorial: false,
            tutorial_step: TutorialStep::default(),
            show_animations: true,
            auto_suggest: true,
            git_ref_mtime: false,
            flycomp: flycomp::FlycompSettings::default(),
            suggestion_sort_order: SuggestionSortOrder::default(),
            fuzzy_mode: FuzzyMode::default(),
            num_suggestion_rows: 12,
            show_inline_history: true,
            auto_close_chars: true,
            select_with_mouse: true,
            cursor_config: CursorConfig::default(),
            mouse_mode: MouseMode::default(),
            agent_commands: HashMap::default(),
            custom_animations: HashMap::default(),
            custom_prompt_widgets: HashMap::default(),
            matrix_animation: MatrixAnimation::default(),
            frame_rate: 24,
            idle_frame_rate: 0.2,
            send_shell_integration_codes: ShellIntegrationLevel::default(),
            enable_extended_key_codes: true,
            enable_easter_eggs: true,
            colour_theme: ColourTheme::default(),
            colour_palette: Palette::default(),
            keybindings: Vec::default(),
            key_remappings: Vec::default(),
            clear_default_keybindings: false,
            key_debug: false,
            mouse_debug: false,
            mouse_change_shape: true,
            history_jsonl_path: crate::history::default_jsonl_path(),
            last_app_closed_at: None,
            initial_buffer: None,
            resize_logic: ResizeLogic::default(),
            delayed_startup_ms: 150,
            history_backend: HistoryBackend::default(),
        }
    }
}

struct GlobalSettings(std::cell::UnsafeCell<Settings>);

// SAFETY: only ever touched from Bash's main thread; flyline spawns no threads
// and `spawn_subshell` forks. A lock here would deadlock rather than serialise,
// because Bash re-enters the `flyline` builtin on that same thread.
unsafe impl Sync for GlobalSettings {}

static GLOBAL_SETTINGS: std::sync::LazyLock<GlobalSettings> =
    std::sync::LazyLock::new(|| GlobalSettings(std::cell::UnsafeCell::new(Settings::default())));

/// Returns a mutable reference to the process-wide global [`Settings`].
///
/// # Safety / Aliasing Rule
/// Borrows must be short-lived and MUST NOT be held across calls into
/// Bash FFI functions (`evaluate_shell_string`, `decode_prompt`, etc.),
/// because Bash may re-enter `flyline` on the same thread.
pub fn settings() -> &'static mut Settings {
    // SAFETY: single-threaded on Bash's main thread; see `GlobalSettings`'s `Sync` impl.
    unsafe { &mut *GLOBAL_SETTINGS.0.get() }
}

/// Resets the process-wide [`Settings`] to [`Settings::default`].
pub(crate) fn reset_settings() {
    *settings() = Settings::default();
}

/// A single diff entry between current session settings and default settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingDiffEntry {
    /// Setting name / field.
    pub name: String,
    /// String representation of the current value.
    pub current: String,
    /// String representation of the default value.
    pub default: String,
    /// Whether the setting is currently at its default value.
    pub is_default: bool,
}

impl Settings {
    /// Computes the diff between this `Settings` instance and `Settings::default()`.
    pub fn diff(&self) -> Vec<SettingDiffEntry> {
        let curr_val = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        let def_val = serde_json::to_value(Settings::default()).unwrap_or(serde_json::Value::Null);

        let mut curr_flat = Vec::new();
        let mut def_flat = Vec::new();
        flatten_json("", &curr_val, &mut curr_flat);
        flatten_json("", &def_val, &mut def_flat);

        let def_map: std::collections::HashMap<_, _> = def_flat.into_iter().collect();

        let mut entries = Vec::new();
        for (key, val) in curr_flat {
            let d_val = def_map.get(&key).unwrap_or(&serde_json::Value::Null);
            entries.push(SettingDiffEntry {
                name: key,
                current: format_json_val(&val),
                default: format_json_val(d_val),
                is_default: &val == d_val,
            });
        }
        entries
    }
}

fn flatten_json(prefix: &str, val: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
    match val {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_json(&key, v, out);
            }
        }
        _ => {
            if !prefix.is_empty() {
                out.push((prefix.to_string(), val.clone()));
            }
        }
    }
}

fn format_json_val(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "-".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
    }
}

fn serialize_agent_commands<S>(
    commands: &HashMap<Option<String>, AgentModeCommand>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(commands.len()))?;
    for (k, v) in commands {
        let key_str = k.as_deref().unwrap_or("<default>");
        map.serialize_entry(key_str, v)?;
    }
    map.end()
}

fn serialize_keybindings<S>(bindings: &[actions::Binding], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(bindings.len()))?;
    for b in bindings {
        seq.serialize_element(&b.display())?;
    }
    seq.end()
}

fn serialize_key_remappings<S>(
    remappings: &[actions::KeyRemap],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(remappings.len()))?;
    for r in remappings {
        seq.serialize_element(&r.display())?;
    }
    seq.end()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_diff_default_has_no_non_defaults() {
        let settings = Settings::default();
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert!(
            changed.is_empty(),
            "Expected no changed settings on default Settings, got: {:?}",
            changed
        );
    }

    #[test]
    fn test_settings_diff_detects_changed_editor_setting() {
        let settings = Settings {
            auto_close_chars: false,
            num_suggestion_rows: 8,
            ..Settings::default()
        };
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 2);
        assert_eq!(changed[0].name, "num_suggestion_rows");
        assert_eq!(changed[0].current, "8");
        assert_eq!(changed[0].default, "12");
        assert_eq!(changed[1].name, "auto_close_chars");
        assert_eq!(changed[1].current, "false");
        assert_eq!(changed[1].default, "true");
    }

    #[test]
    fn test_settings_diff_detects_custom_palette() {
        let mut settings = Settings::default();
        settings.colour_palette.set(
            crate::palette::PaletteStyleKind::RecognisedCommand,
            ratatui::style::Style::default().fg(ratatui::style::Color::Yellow),
        );
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "palette.recognised_command.fg");
        assert_eq!(changed[0].current, "Yellow");
    }

    #[test]
    fn test_settings_diff_detects_changed_history_jsonl_path() {
        let mut settings = Settings::default();
        settings.history_jsonl_path = std::path::PathBuf::from("/tmp/test.jsonl");
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "history.jsonl_path");
        assert_eq!(changed[0].current, "/tmp/test.jsonl");
    }

    #[test]
    fn test_settings_diff_detects_custom_keybinding() {
        let mut settings = Settings::default();
        let binding = actions::Binding::try_new_from_strs("ctrl+a", "always=selectAll").unwrap();
        settings.keybindings.push(binding);
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "user_keybindings");
        assert!(
            changed[0].current.contains("Ctrl+a always=selectAll"),
            "Expected pretty binding string, got: {}",
            changed[0].current
        );
    }

    /// A re-entrant builtin call must reach the same settings the app is holding.
    /// The only test that touches the global, so it cannot race the others.
    #[test]
    fn reentrant_handles_share_one_settings_instance() {
        let app_view = settings();
        app_view.frame_rate = 11;
        settings().frame_rate = 30;
        assert_eq!(app_view.frame_rate, 30);
    }

    #[test]
    fn test_settings_diff_detects_changed_idle_frame_rate() {
        let settings = Settings {
            idle_frame_rate: 0.5,
            ..Settings::default()
        };
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "idle_frame_rate");
        assert_eq!(changed[0].current, "0.5");
        assert_eq!(changed[0].default, "0.2");
    }

    #[test]
    fn test_settings_diff_detects_changed_delayed_startup_ms() {
        let mut settings = Settings::default();
        settings.delayed_startup_ms = 300;
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].name, "delayed_startup_ms");
        assert_eq!(changed[0].current, "300");
        assert_eq!(changed[0].default, "150");
    }
}
