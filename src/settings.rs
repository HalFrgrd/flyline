use std::collections::HashMap;

use crate::app::actions;
use crate::content_builder::TaggedSpan;
use crate::cursor::CursorConfig;
use crate::history::HistoryManager;
use crate::palette::Palette;
use crate::tutorial::TutorialStep;
use clap::ValueEnum;

/// Which theme the user has configured for the colour palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColourTheme {
    /// Dark-terminal preset (the original flyline palette). This is the default.
    #[default]
    Dark,
    /// Light-terminal preset.
    Light,
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

/// A prompt widget that shows different text depending on whether mouse capture is enabled.
#[derive(Debug, Clone)]
pub struct PromptWidgetMouseMode {
    /// Name used as placeholder in prompt strings (e.g., `FLYLINE_MOUSE_MODE`).
    pub name: String,
    /// Text shown when mouse capture is enabled.
    pub enabled_text: String,
    /// Text shown when mouse capture is disabled.
    pub disabled_text: String,
}

/// A prompt widget that copies the current command buffer to the clipboard when clicked.
#[derive(Debug, Clone)]
pub struct PromptWidgetCopyBuffer {
    /// Name used as placeholder in prompt strings (e.g., `FLYLINE_COPY_BUFFER`).
    pub name: String,
    /// Text shown in the prompt.
    pub text: String,
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

/// A prompt widget that shows how long ago the flyline app last closed.
///
/// The `format` field controls the output:
/// * `"five-chars"` (the default) – compact 5-character duration string
///   produced by [`crate::content_utils::duration_to_5chars`] (e.g. `"01min"`, `"02hou"`).
/// * Any other string – treated as a Chrono `strftime` format applied to the
///   absolute local [`chrono::DateTime`] when the app last closed (e.g. `"%H:%M:%S"`).
///
/// When the app has not yet closed in the current session the widget renders
/// `" now "` (five-chars mode) or the current time (format mode).
#[derive(Debug, Clone)]
pub struct PromptWidgetLastCommandDuration {
    /// Name used as placeholder in prompt strings (e.g., `FLYLINE_LAST_COMMAND_DURATION`).
    pub name: String,
    /// Output format: `"five-chars"` or a Chrono strftime format string.
    pub format: String,
}

/// A custom prompt widget registered with `flyline create-prompt-widget`.
#[derive(Debug, Clone)]
pub enum PromptWidget {
    MouseMode(PromptWidgetMouseMode),
    CopyBuffer(PromptWidgetCopyBuffer),
    Custom(PromptWidgetCustom),
    LastCommandDuration(PromptWidgetLastCommandDuration),
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
#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq, Default)]
pub enum MouseMode {
    /// Never capture mouse events.
    Disabled,
    /// Mouse capture is on by default; toggled when Escape is pressed.
    Simple,
    /// Mouse capture is on by default with automatic management: disabled on scroll or when the
    /// user clicks above the viewport, re-enabled on any keypress or when focus is regained.
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
    /// Configurable colour palette for UI elements.
    pub colour_palette: Palette,
    /// User defined keybindings
    pub keybindings: Vec<actions::Binding>,
    /// User defined key remappings (applied before matching bindings).
    pub key_remappings: Vec<actions::KeyRemap>,
    /// Show the last key event and dispatched action above the prompt.
    pub key_debug: bool,
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
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zsh_history_path: None,
            run_tutorial: false,
            tutorial_step: TutorialStep::default(),
            show_animations: true,
            show_inline_history: true,
            auto_close_chars: true,
            select_with_mouse: true,
            cursor_config: CursorConfig::default(),
            mouse_mode: MouseMode::default(),
            agent_commands: HashMap::new(),
            custom_animations: HashMap::new(),
            custom_prompt_widgets: HashMap::new(),
            matrix_animation: MatrixAnimation::default(),
            frame_rate: 24,
            send_shell_integration_codes: ShellIntegrationLevel::default(),
            enable_extended_key_codes: true,
            colour_palette: Palette::default(),
            keybindings: Vec::new(),
            key_remappings: Vec::new(),
            key_debug: false,
            cancelled_command_history_manager: HistoryManager::new_empty(),
            agent_prompt_history_manager: HistoryManager::new_empty(),
            last_app_closed_at: None,
            #[cfg(feature = "integration-tests")]
            run_tab_completion_tests: false,
        }
    }
}
