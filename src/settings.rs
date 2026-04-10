use std::collections::HashMap;

use crate::app::actions;
use crate::cursor::CursorConfig;
use crate::history::HistoryManager;
use crate::palette::Palette;
use crate::tutorial::TutorialStep;
use clap::ValueEnum;

/// Which theme the user has configured for the colour palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorTheme {
    /// Dark-terminal preset (the original flyline palette). This is the default.
    #[default]
    Dark,
    /// Light-terminal preset.
    Light,
}

/// A single custom prompt animation registered with `flyline create-prompt-anim`.
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

/// What to show as a placeholder while a non-blocking (or timed-out blocking)
/// custom widget command is still running.
#[derive(Debug, Clone)]
pub enum Placeholder {
    /// Show N spaces.
    Spaces(usize),
    /// Show the previous output of the command (empty on the very first run).
    Prev,
}

/// A prompt widget that runs a shell command and displays its output.
#[derive(Debug, Clone)]
pub struct PromptWidgetCustom {
    /// Name used as placeholder in prompt strings (e.g., `CUSTOM_WIDGET1`).
    pub name: String,
    /// Command (and arguments) to run.
    pub command: Vec<String>,
    /// When `Some(n)`, wait up to `n` milliseconds for the command to finish
    /// before rendering the first prompt frame.  `Some(i32::MAX)` means wait
    /// indefinitely.  `None` means the command always runs in the background.
    pub block: Option<i32>,
    /// What to show while the command is running (or has timed out).
    pub placeholder: Option<Placeholder>,
    /// Most recent successful output of the command; shared across clones so
    /// that the `Placeholder::Prev` option can pick it up on subsequent renders.
    pub prev_output: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

/// A custom prompt widget registered with `flyline create-prompt-widget`.
#[derive(Debug, Clone)]
pub enum PromptWidget {
    MouseMode(PromptWidgetMouseMode),
    Custom(PromptWidgetCustom),
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
    OnlyPromptPos,
    /// Send the full set of shell integration codes: prompt positions, execution
    /// start/end codes, and cursor-position reporting.  This is the default.
    #[default]
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
    /// Cursor appearance and animation settings (set via `flyline set-cursor`).
    pub cursor_config: CursorConfig,
    /// Mouse capture mode.
    pub mouse_mode: MouseMode,
    /// Agent-mode commands keyed by optional trigger prefix.
    /// - `None` key: the default command invoked via Alt+Enter (no prefix match needed).
    /// - `Some(prefix)` key: activated when the user presses Enter and the buffer starts
    ///   with `prefix`; the prefix is stripped before the buffer is sent to the command.
    pub agent_commands: HashMap<Option<String>, AgentModeCommand>,
    /// Custom prompt animations registered with `flyline create-prompt-anim`.
    pub custom_animations: HashMap<String, PromptAnimation>,
    /// Custom prompt widgets registered with `flyline create-prompt-widget`.
    pub custom_prompt_widgets: HashMap<String, PromptWidget>,
    /// Run matrix animation in the terminal background.
    pub matrix_animation: MatrixAnimation,
    /// Render frame rate in frames per second (1–120).
    pub frame_rate: u8,
    /// Shell integration escape codes level (OSC 133 / OSC 633).
    pub send_shell_integration_codes: ShellIntegrationLevel,
    /// Configurable colour palette for UI elements.
    pub color_palette: Palette,
    /// Which colour theme the user has selected (dark or light).
    pub color_theme: ColorTheme,
    /// User defined keybindings
    pub keybindings: Vec<actions::Binding>,
    /// User defined key remappings (applied before matching bindings).
    pub key_remappings: Vec<actions::KeyRemap>,
    /// Tracks commands that were cancelled via Ctrl+C (non-empty buffer).
    pub cancelled_command_history_manager: HistoryManager,
    /// Tracks prompts that were submitted to agent mode.
    pub agent_prompt_history_manager: HistoryManager,
    /// Whether to run tab completion tests (used for integration testing).
    #[cfg(feature = "integration-tests")]
    pub run_tab_completion_tests: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zsh_history_path: None,
            run_tutorial: false,
            tutorial_step: TutorialStep::NotRunning,
            show_animations: true,
            show_inline_history: true,
            auto_close_chars: true,
            cursor_config: CursorConfig::default(),
            mouse_mode: MouseMode::Smart,
            agent_commands: HashMap::new(),
            custom_animations: HashMap::new(),
            custom_prompt_widgets: HashMap::new(),
            matrix_animation: MatrixAnimation::Off,
            frame_rate: 30,
            send_shell_integration_codes: ShellIntegrationLevel::Full,
            color_palette: Palette::default(),
            color_theme: ColorTheme::Dark,
            keybindings: Vec::new(),
            key_remappings: Vec::new(),
            cancelled_command_history_manager: HistoryManager::new_empty(),
            agent_prompt_history_manager: HistoryManager::new_empty(),
            #[cfg(feature = "integration-tests")]
            run_tab_completion_tests: false,
        }
    }
}
