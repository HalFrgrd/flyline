use std::collections::HashMap;

use crate::app::actions;
use crate::content::TaggedSpan;
use crate::cursor::CursorConfig;
use crate::history::HistoryManager;
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

/// A custom prompt widget registered with `flyline create-prompt-widget`.
#[derive(Debug, Clone)]
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
    /// The placeholder name that is replaced inside prompt strings (PS1, RPS1, PS1_FILL, PS2).
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
#[derive(Debug, Clone, Default)]
pub enum Placeholder {
    /// Show N spaces.
    Spaces(usize),
    /// Show the previous output of the command (empty on the very first run).
    #[default]
    Prev,
}

/// A prompt widget that runs a shell command and displays its output.
#[derive(Debug, Clone)]
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
    pub prev_output: std::sync::Arc<std::sync::Mutex<Vec<TaggedSpan<'static>>>>,
}

/// A configured agent-mode command with its optional system prompt.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug)]
pub struct Settings {
    /// Optional path to the Zsh history file. When `None`, Zsh history is not loaded.
    /// When `Some`, Zsh history is loaded in addition to Bash history; an empty string or no
    /// value means use the default path (`$HOME/.zsh_history`).
    pub zsh_history_path: Option<String>,
    /// Whether the interactive tutorial is active.
    pub run_tutorial: bool,
    /// Current tutorial step.
    pub tutorial_step: TutorialStep,
    /// Whether to show all animations (cursor movement, cursor fading, dynamic time).
    pub show_animations: bool,
    /// Whether to show inline history suggestions.
    pub show_inline_history: bool,
    /// Whether to auto-start tab completion suggestions as you type.
    pub auto_suggest: bool,
    /// Settings for flycomp shell completion synthesis.
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
    pub cursor_config: CursorConfig,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Agent-mode commands keyed by optional trigger prefix.
    /// - `None` key: the default command invoked via Alt+Enter (no prefix match needed).
    /// - `Some(prefix)` key: activated when the user presses Enter and the buffer starts
    ///   with `prefix`; the prefix is stripped before the buffer is sent to the command.
    pub agent_commands: HashMap<Option<String>, AgentModeCommand>,
    /// Custom prompt animations registered with `flyline create-prompt-widget animation`.
    pub custom_animations: HashMap<String, PromptAnimation>,
    /// Custom prompt widgets registered with `flyline create-prompt-widget`.
    pub custom_prompt_widgets: HashMap<String, PromptWidget>,
    /// Run matrix animation in the terminal background.
    pub matrix_animation: MatrixAnimation,
    /// Render frame rate in frames per second (1–120).
    pub frame_rate: u8,
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
    /// Configurable colour palette for UI elements.
    pub colour_palette: Palette,
    /// User defined keybindings
    pub keybindings: Vec<actions::Binding>,
    /// User defined key remappings (applied before matching bindings).
    pub key_remappings: Vec<actions::KeyRemap>,
    /// Whether built-in default keybindings should be ignored.
    pub clear_default_keybindings: bool,
    /// Show the last key event and dispatched action above the prompt.
    pub key_debug: bool,
    /// Show the last mouse event above the prompt.
    pub mouse_debug: bool,
    /// Whether to change the mouse cursor shape depending on what is hovered.
    pub mouse_change_shape: bool,
    /// Tracks commands that were cancelled via Ctrl+C (non-empty buffer).
    pub cancelled_command_history_manager: HistoryManager,
    /// Tracks prompts that were submitted to agent mode.
    pub agent_prompt_history_manager: HistoryManager,
    /// Timestamp of the most recent flyline app session close.
    ///
    /// Set to `Some(Instant::now())` immediately after each `app::get_command`
    /// call returns. Used by the `last-command-duration` prompt widget to
    /// compute and display the elapsed time since the last command.
    pub last_app_closed_at: Option<std::time::Instant>,
    /// Initial buffer content to pre-fill the command line when Flyline starts.
    pub initial_buffer: Option<String>,
    /// Resize logic strategy for cursor placement on window resize.
    pub resize_logic: ResizeLogic,
    /// Configured history storage backend (flyline, bash, or atuin).
    pub history_backend: HistoryBackend,
    /// Long-lived main command history manager.
    pub history_manager: HistoryManager,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zsh_history_path: None,
            run_tutorial: false,
            tutorial_step: TutorialStep::default(),
            show_animations: true,
            auto_suggest: true,
            flycomp: flycomp::FlycompSettings::default(),
            suggestion_sort_order: SuggestionSortOrder::default(),
            fuzzy_mode: FuzzyMode::default(),
            num_suggestion_rows: 15,
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
            send_shell_integration_codes: ShellIntegrationLevel::default(),
            enable_extended_key_codes: true,
            enable_easter_eggs: true,
            colour_palette: Palette::default(),
            keybindings: Vec::default(),
            key_remappings: Vec::default(),
            clear_default_keybindings: false,
            key_debug: false,
            mouse_debug: false,
            mouse_change_shape: true,
            cancelled_command_history_manager: HistoryManager::default(),
            agent_prompt_history_manager: HistoryManager::default(),
            last_app_closed_at: None,
            initial_buffer: None,
            resize_logic: ResizeLogic::default(),
            history_backend: HistoryBackend::default(),
            history_manager: HistoryManager::default(),
        }
    }
}

/// A single diff entry between current session settings and default settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingDiffEntry {
    /// Category grouping (e.g. "Editor", "Suggestions", "Mouse & Display", "Cursor", "Keybindings", "Colors & Theme", "Widgets & AI", "History & Integration").
    pub category: &'static str,
    /// Setting name (e.g. "auto-close-chars").
    pub name: String,
    /// Human-readable string representation of the current value.
    pub current: String,
    /// Human-readable string representation of the default value.
    pub default: String,
    /// Whether the setting is currently at its default value.
    pub is_default: bool,
    /// Equivalent flyline CLI command to set this value.
    pub cli_command: Option<String>,
}

impl Settings {
    /// Computes the diff between this `Settings` instance and `Settings::default()`.
    pub fn diff(&self) -> Vec<SettingDiffEntry> {
        let def = Settings::default();
        let mut entries = Vec::new();

        // ── 1. Editor & Input ──────────────────────────────────────────
        entries.push(SettingDiffEntry {
            category: "Editor",
            name: "auto-close-chars".to_string(),
            current: self.auto_close_chars.to_string(),
            default: def.auto_close_chars.to_string(),
            is_default: self.auto_close_chars == def.auto_close_chars,
            cli_command: Some(format!(
                "flyline editor --auto-close-chars {}",
                self.auto_close_chars
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Editor",
            name: "show-inline-history".to_string(),
            current: self.show_inline_history.to_string(),
            default: def.show_inline_history.to_string(),
            is_default: self.show_inline_history == def.show_inline_history,
            cli_command: Some(format!(
                "flyline editor --show-inline-history {}",
                self.show_inline_history
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Editor",
            name: "select-with-mouse".to_string(),
            current: self.select_with_mouse.to_string(),
            default: def.select_with_mouse.to_string(),
            is_default: self.select_with_mouse == def.select_with_mouse,
            cli_command: Some(format!(
                "flyline editor --select-with-mouse {}",
                self.select_with_mouse
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Editor",
            name: "enable-extended-key-codes".to_string(),
            current: self.enable_extended_key_codes.to_string(),
            default: def.enable_extended_key_codes.to_string(),
            is_default: self.enable_extended_key_codes == def.enable_extended_key_codes,
            cli_command: Some(format!(
                "flyline --enable-extended-key-codes {}",
                self.enable_extended_key_codes
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Editor",
            name: "enable-easter-eggs".to_string(),
            current: self.enable_easter_eggs.to_string(),
            default: def.enable_easter_eggs.to_string(),
            is_default: self.enable_easter_eggs == def.enable_easter_eggs,
            cli_command: Some(format!(
                "flyline --enable-easter-eggs {}",
                self.enable_easter_eggs
            )),
        });

        // ── 2. Suggestions & Completions ───────────────────────────────
        entries.push(SettingDiffEntry {
            category: "Suggestions",
            name: "auto-suggest".to_string(),
            current: self.auto_suggest.to_string(),
            default: def.auto_suggest.to_string(),
            is_default: self.auto_suggest == def.auto_suggest,
            cli_command: Some(format!(
                "flyline suggestions --auto-suggest {}",
                self.auto_suggest
            )),
        });

        let sort_order_str = |s: SuggestionSortOrder| match s {
            SuggestionSortOrder::Mtime => "mtime",
            SuggestionSortOrder::Alphabetical => "alphabetical",
        };
        entries.push(SettingDiffEntry {
            category: "Suggestions",
            name: "sort-order".to_string(),
            current: sort_order_str(self.suggestion_sort_order).to_string(),
            default: sort_order_str(def.suggestion_sort_order).to_string(),
            is_default: self.suggestion_sort_order == def.suggestion_sort_order,
            cli_command: Some(format!(
                "flyline suggestions --sort-order {}",
                sort_order_str(self.suggestion_sort_order)
            )),
        });

        let fuzzy_mode_str = |f: FuzzyMode| match f {
            FuzzyMode::All => "all",
            FuzzyMode::None => "none",
            FuzzyMode::FolderPrefixes => "folder-prefixes",
        };
        entries.push(SettingDiffEntry {
            category: "Suggestions",
            name: "fuzzy-mode".to_string(),
            current: fuzzy_mode_str(self.fuzzy_mode).to_string(),
            default: fuzzy_mode_str(def.fuzzy_mode).to_string(),
            is_default: self.fuzzy_mode == def.fuzzy_mode,
            cli_command: Some(format!(
                "flyline suggestions set-fuzzy-mode {}",
                fuzzy_mode_str(self.fuzzy_mode)
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Suggestions",
            name: "num-suggestion-rows".to_string(),
            current: self.num_suggestion_rows.to_string(),
            default: def.num_suggestion_rows.to_string(),
            is_default: self.num_suggestion_rows == def.num_suggestion_rows,
            cli_command: Some(format!(
                "flyline suggestions --num-suggestion-rows {}",
                self.num_suggestion_rows
            )),
        });

        // ── 3. Mouse & Display ─────────────────────────────────────────
        let mouse_mode_str = |m: MouseMode| match m {
            MouseMode::Disabled => "disabled",
            MouseMode::Simple => "simple",
            MouseMode::Smart => "smart",
        };
        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "mouse-mode".to_string(),
            current: mouse_mode_str(self.mouse_mode).to_string(),
            default: mouse_mode_str(def.mouse_mode).to_string(),
            is_default: self.mouse_mode == def.mouse_mode,
            cli_command: Some(format!(
                "flyline mouse --mode {}",
                mouse_mode_str(self.mouse_mode)
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "mouse-change-shape".to_string(),
            current: self.mouse_change_shape.to_string(),
            default: def.mouse_change_shape.to_string(),
            is_default: self.mouse_change_shape == def.mouse_change_shape,
            cli_command: Some(format!(
                "flyline mouse --change-shape {}",
                self.mouse_change_shape
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "mouse-debug".to_string(),
            current: self.mouse_debug.to_string(),
            default: def.mouse_debug.to_string(),
            is_default: self.mouse_debug == def.mouse_debug,
            cli_command: Some(format!("flyline mouse --debug {}", self.mouse_debug)),
        });

        let matrix_str = |m: &MatrixAnimation| match m {
            MatrixAnimation::Off => "off".to_string(),
            MatrixAnimation::On => "on".to_string(),
            MatrixAnimation::IdleSecs(s) => format!("{s}s"),
        };
        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "matrix-animation".to_string(),
            current: matrix_str(&self.matrix_animation),
            default: matrix_str(&def.matrix_animation),
            is_default: self.matrix_animation == def.matrix_animation,
            cli_command: Some(match self.matrix_animation {
                MatrixAnimation::Off => "flyline --matrix-animation off".to_string(),
                MatrixAnimation::On => "flyline --matrix-animation on".to_string(),
                MatrixAnimation::IdleSecs(s) => format!("flyline --matrix-animation {s}"),
            }),
        });

        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "frame-rate".to_string(),
            current: self.frame_rate.to_string(),
            default: def.frame_rate.to_string(),
            is_default: self.frame_rate == def.frame_rate,
            cli_command: Some(format!("flyline --set-frame-rate {}", self.frame_rate)),
        });

        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "show-animations".to_string(),
            current: self.show_animations.to_string(),
            default: def.show_animations.to_string(),
            is_default: self.show_animations == def.show_animations,
            cli_command: Some(format!(
                "flyline --show-animations {}",
                self.show_animations
            )),
        });

        let resize_str = |r: ResizeLogic| match r {
            ResizeLogic::Default => "default",
            ResizeLogic::AutoCleared => "auto-cleared",
            ResizeLogic::ReflowedApartFromCursor => "reflowed-apart-from-cursor",
            ResizeLogic::ReflowedAll => "reflowed-all",
            ResizeLogic::ReflowedAllWhitespaceTrimmed => "reflowed-all-whitespace-trimmed",
            ResizeLogic::DontMoveCursor => "dont-move-cursor",
        };
        entries.push(SettingDiffEntry {
            category: "Mouse & Display",
            name: "resize-logic".to_string(),
            current: resize_str(self.resize_logic).to_string(),
            default: resize_str(def.resize_logic).to_string(),
            is_default: self.resize_logic == def.resize_logic,
            cli_command: Some(format!(
                "flyline --set-resize-logic {}",
                resize_str(self.resize_logic)
            )),
        });

        // ── 4. Cursor Appearance & Animations ──────────────────────────
        let cursor_backend_str = if self.cursor_config.is_backend_unset() {
            "default (auto)".to_string()
        } else {
            match self.cursor_config.backend() {
                crate::cursor::CursorBackend::Flyline => "flyline".to_string(),
                crate::cursor::CursorBackend::Terminal => "terminal".to_string(),
            }
        };
        let is_cursor_backend_default = self.cursor_config.is_backend_unset();
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.backend".to_string(),
            current: cursor_backend_str,
            default: "default (auto)".to_string(),
            is_default: is_cursor_backend_default,
            cli_command: if is_cursor_backend_default {
                None
            } else {
                Some(format!(
                    "flyline set-cursor --backend {}",
                    match self.cursor_config.backend() {
                        crate::cursor::CursorBackend::Flyline => "flyline",
                        crate::cursor::CursorBackend::Terminal => "terminal",
                    }
                ))
            },
        });

        let interp_str = |opt: Option<f32>| match opt {
            Some(v) => format!("{v}"),
            None => "none".to_string(),
        };
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.interpolate".to_string(),
            current: interp_str(self.cursor_config.interpolate),
            default: interp_str(def.cursor_config.interpolate),
            is_default: self.cursor_config.interpolate == def.cursor_config.interpolate,
            cli_command: Some(format!(
                "flyline set-cursor --interpolate {}",
                interp_str(self.cursor_config.interpolate)
            )),
        });

        let easing_str = |e: crate::cursor::CursorEasing| e.as_ref().to_lowercase();
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.interpolate-easing".to_string(),
            current: easing_str(self.cursor_config.interpolate_easing),
            default: easing_str(def.cursor_config.interpolate_easing),
            is_default: self.cursor_config.interpolate_easing
                == def.cursor_config.interpolate_easing,
            cli_command: Some(format!(
                "flyline set-cursor --interpolate-easing {}",
                easing_str(self.cursor_config.interpolate_easing)
            )),
        });

        let cursor_style_str = |st: &crate::cursor::CursorStyleConfig| match st {
            crate::cursor::CursorStyleConfig::Default => "default".to_string(),
            crate::cursor::CursorStyleConfig::Reverse => "reverse".to_string(),
            crate::cursor::CursorStyleConfig::Custom(s) => crate::palette::style_to_rich_string(*s),
        };
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.style".to_string(),
            current: cursor_style_str(&self.cursor_config.style),
            default: cursor_style_str(&def.cursor_config.style),
            is_default: self.cursor_config.style == def.cursor_config.style,
            cli_command: Some(format!(
                "flyline set-cursor --style \"{}\"",
                cursor_style_str(&self.cursor_config.style)
            )),
        });

        let cursor_effect_str = |eff: crate::cursor::CursorEffect| match eff {
            crate::cursor::CursorEffect::Fade => "fade",
            crate::cursor::CursorEffect::Blink => "blink",
            crate::cursor::CursorEffect::None => "none",
        };
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.effect".to_string(),
            current: cursor_effect_str(self.cursor_config.effect).to_string(),
            default: cursor_effect_str(def.cursor_config.effect).to_string(),
            is_default: self.cursor_config.effect == def.cursor_config.effect,
            cli_command: Some(format!(
                "flyline set-cursor --effect {}",
                cursor_effect_str(self.cursor_config.effect)
            )),
        });

        let speed_is_default =
            (self.cursor_config.effect_speed - def.cursor_config.effect_speed).abs() < 1e-4;
        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.effect-speed".to_string(),
            current: format!("{:.1}", self.cursor_config.effect_speed),
            default: format!("{:.1}", def.cursor_config.effect_speed),
            is_default: speed_is_default,
            cli_command: Some(format!(
                "flyline set-cursor --effect-speed {:.1}",
                self.cursor_config.effect_speed
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Cursor",
            name: "cursor.effect-easing".to_string(),
            current: easing_str(self.cursor_config.effect_easing),
            default: easing_str(def.cursor_config.effect_easing),
            is_default: self.cursor_config.effect_easing == def.cursor_config.effect_easing,
            cli_command: Some(format!(
                "flyline set-cursor --effect-easing {}",
                easing_str(self.cursor_config.effect_easing)
            )),
        });

        // ── 5. Keybindings & Remappings ────────────────────────────────
        entries.push(SettingDiffEntry {
            category: "Keybindings",
            name: "clear-default-keybindings".to_string(),
            current: self.clear_default_keybindings.to_string(),
            default: def.clear_default_keybindings.to_string(),
            is_default: self.clear_default_keybindings == def.clear_default_keybindings,
            cli_command: Some(format!(
                "flyline key --clear-defaults {}",
                self.clear_default_keybindings
            )),
        });

        entries.push(SettingDiffEntry {
            category: "Keybindings",
            name: "key-debug".to_string(),
            current: self.key_debug.to_string(),
            default: def.key_debug.to_string(),
            is_default: self.key_debug == def.key_debug,
            cli_command: Some(format!("flyline key --debug {}", self.key_debug)),
        });

        for binding in &self.keybindings {
            let key_str = binding.format_keys(&self.key_remappings);
            let action_str = binding.format_actions();
            let ctx_str = binding.context().display();
            entries.push(SettingDiffEntry {
                category: "Keybindings",
                name: format!("bind {key_str}"),
                current: format!("{ctx_str}={action_str}"),
                default: "-".to_string(),
                is_default: false,
                cli_command: Some(format!(
                    "flyline key bind {} \"{}={}\"",
                    binding.format_keys(&[]),
                    ctx_str,
                    action_str
                )),
            });
        }

        for remap in &self.key_remappings {
            let from_str = remap.from_display();
            let to_str = remap.to_display();
            entries.push(SettingDiffEntry {
                category: "Keybindings",
                name: format!("remap {from_str}"),
                current: format!("{from_str} -> {to_str}"),
                default: "-".to_string(),
                is_default: false,
                cli_command: Some(format!("flyline key remap {from_str} {to_str}")),
            });
        }

        // ── 6. Colours & Theme ─────────────────────────────────────────
        use strum::IntoEnumIterator;
        for kind in crate::palette::PaletteStyleKind::iter() {
            let curr_style = self.colour_palette.get(kind);
            let def_style = def.colour_palette.get(kind);
            let is_match = curr_style == def_style;
            let curr_rich = crate::palette::style_to_rich_string(curr_style);
            let def_rich = crate::palette::style_to_rich_string(def_style);
            entries.push(SettingDiffEntry {
                category: "Colors & Theme",
                name: kind.to_string(),
                current: curr_rich.clone(),
                default: def_rich,
                is_default: is_match,
                cli_command: Some(format!("flyline set-style {}=\"{}\"", kind, curr_rich)),
            });
        }

        // ── 7. Widgets & AI Mode ───────────────────────────────────────
        for (prefix, cmd) in &self.agent_commands {
            let prefix_label = prefix
                .as_ref()
                .map(|p| format!(" (trigger: {:?})", p))
                .unwrap_or_default();
            let cmd_str = cmd.command.join(" ");
            let prompt_opt = cmd
                .system_prompt
                .as_ref()
                .map(|p| format!(" --system-prompt {:?}", p))
                .unwrap_or_default();
            let trigger_opt = prefix
                .as_ref()
                .map(|p| format!(" --trigger-prefix {:?}", p))
                .unwrap_or_default();
            entries.push(SettingDiffEntry {
                category: "Widgets & AI",
                name: format!("agent-mode{prefix_label}"),
                current: cmd_str.clone(),
                default: "-".to_string(),
                is_default: false,
                cli_command: Some(format!(
                    "flyline set-agent-mode{} --command '{}'{}",
                    trigger_opt, cmd_str, prompt_opt
                )),
            });
        }

        for (name, anim) in &self.custom_animations {
            let ping_pong_str = if anim.ping_pong { " (ping-pong)" } else { "" };
            entries.push(SettingDiffEntry {
                category: "Widgets & AI",
                name: format!("animation: {name}"),
                current: format!(
                    "{} fps, {} frames{}",
                    anim.fps,
                    anim.frames.len(),
                    ping_pong_str
                ),
                default: "-".to_string(),
                is_default: false,
                cli_command: Some(format!(
                    "flyline create-prompt-widget animation --name \"{}\" --fps {}{}",
                    name,
                    anim.fps,
                    if anim.ping_pong { " --ping-pong" } else { "" }
                )),
            });
        }

        for (name, widget) in &self.custom_prompt_widgets {
            let desc = match widget {
                PromptWidget::MouseMode {
                    enabled_text,
                    disabled_text,
                    ..
                } => {
                    format!(
                        "mouse-mode (on: {:?}, off: {:?})",
                        enabled_text, disabled_text
                    )
                }
                PromptWidget::CopyBuffer { text, .. } => format!("copy-buffer ({:?})", text),
                PromptWidget::Custom(c) => format!("custom ({:?})", c.command.join(" ")),
                PromptWidget::LastCommandDuration { .. } => "last-command-duration".to_string(),
                PromptWidget::LeaderMode {
                    active_text,
                    inactive_text,
                    ..
                } => {
                    format!(
                        "leader-mode (active: {:?}, inactive: {:?})",
                        active_text, inactive_text
                    )
                }
                PromptWidget::BufferLineNumber { .. } => "buffer-line-number".to_string(),
            };
            entries.push(SettingDiffEntry {
                category: "Widgets & AI",
                name: format!("widget: {name}"),
                current: desc,
                default: "-".to_string(),
                is_default: false,
                cli_command: None,
            });
        }

        // ── 8. History & Integration ───────────────────────────────────
        let backend_str = match self.history_backend {
            HistoryBackend::Flyline => "flyline",
            HistoryBackend::Bash => "bash",
        };
        let def_backend_str = match def.history_backend {
            HistoryBackend::Flyline => "flyline",
            HistoryBackend::Bash => "bash",
        };
        entries.push(SettingDiffEntry {
            category: "History & Integration",
            name: "history-backend".to_string(),
            current: backend_str.to_string(),
            default: def_backend_str.to_string(),
            is_default: self.history_backend == def.history_backend,
            cli_command: Some(format!("flyline history --backend {}", backend_str)),
        });

        entries.push(SettingDiffEntry {
            category: "History & Integration",
            name: "load-zsh-history".to_string(),
            current: self
                .zsh_history_path
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            default: "-".to_string(),
            is_default: self.zsh_history_path == def.zsh_history_path,
            cli_command: self
                .zsh_history_path
                .as_ref()
                .map(|p| format!("flyline --load-zsh-history {}", p)),
        });

        let shell_int_str = |s: ShellIntegrationLevel| match s {
            ShellIntegrationLevel::None => "none",
            ShellIntegrationLevel::OnlyPromptPos => "only-prompt-pos",
            ShellIntegrationLevel::Full => "full",
        };
        entries.push(SettingDiffEntry {
            category: "History & Integration",
            name: "send-shell-integration-codes".to_string(),
            current: shell_int_str(self.send_shell_integration_codes).to_string(),
            default: shell_int_str(def.send_shell_integration_codes).to_string(),
            is_default: self.send_shell_integration_codes == def.send_shell_integration_codes,
            cli_command: Some(format!(
                "flyline --send-shell-integration-codes {}",
                shell_int_str(self.send_shell_integration_codes)
            )),
        });

        entries.push(SettingDiffEntry {
            category: "History & Integration",
            name: "run-tutorial".to_string(),
            current: self.run_tutorial.to_string(),
            default: def.run_tutorial.to_string(),
            is_default: self.run_tutorial == def.run_tutorial,
            cli_command: Some(format!("flyline run-tutorial {}", self.run_tutorial)),
        });

        entries
    }
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
        let mut settings = Settings::default();
        settings.auto_close_chars = false;
        settings.num_suggestion_rows = 8;
        let diff = settings.diff();
        let changed: Vec<_> = diff.iter().filter(|e| !e.is_default).collect();
        assert_eq!(changed.len(), 2);
        assert_eq!(changed[0].name, "auto-close-chars");
        assert_eq!(changed[0].current, "false");
        assert_eq!(changed[0].default, "true");
        assert_eq!(
            changed[0].cli_command.as_deref(),
            Some("flyline editor --auto-close-chars false")
        );
        assert_eq!(changed[1].name, "num-suggestion-rows");
        assert_eq!(changed[1].current, "8");
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
        assert_eq!(changed[0].name, "recognised-command");
        assert_eq!(changed[0].current, "yellow");
        assert_eq!(
            changed[0].cli_command.as_deref(),
            Some("flyline set-style recognised-command=\"yellow\"")
        );
    }
}
