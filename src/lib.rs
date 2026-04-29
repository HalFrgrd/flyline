use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use clap_complete::{ArgValueCompleter, Shell, generate};
use libc::{c_char, c_int};
use std::sync::Mutex;

#[cfg(feature = "pre_bash_4_4")]
use ctor::ctor;

use crate::{
    app::actions::{self},
    cursor::CursorStyleConfig,
};

#[macro_use]
mod macros;
mod active_suggestions;
mod agent_mode;
mod app;
mod bash_funcs;
mod bash_symbols;
mod command_acceptance;
mod comp_spec_synthesis;
mod content_builder;
mod content_utils;
mod cursor;
mod dparser;
mod history;
mod iter_first_last;
mod kill_on_drop_child;
mod logging;
mod mouse_state;
mod palette;
mod prompt_manager;
mod settings;
mod shell_integration;
mod snake_animation;
mod stateful_sliding_window;
mod tab_completion_context;
mod table;
mod text_buffer;
mod tutorial;
pub mod unicode_helpers;
mod users;

fn get_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .header(
            clap::builder::styling::AnsiColor::Yellow.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
        .usage(
            clap::builder::styling::AnsiColor::Yellow.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
        .literal(
            clap::builder::styling::AnsiColor::Green.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
        .placeholder(clap::builder::styling::AnsiColor::White.on_default())
        .error(
            clap::builder::styling::AnsiColor::Red.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
        .valid(
            clap::builder::styling::AnsiColor::Green.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
        .invalid(
            clap::builder::styling::AnsiColor::Red.on_default()
                | clap::builder::styling::Effects::BOLD,
        )
}

fn parse_matrix_animation(s: &str) -> Result<settings::MatrixAnimation, String> {
    match s {
        "on" => Ok(settings::MatrixAnimation::On),
        "off" => Ok(settings::MatrixAnimation::Off),
        _ => s
            .parse::<u64>()
            .map(settings::MatrixAnimation::IdleSecs)
            .map_err(|_| format!("expected `on`, `off`, or a non-negative integer, got `{s}`")),
    }
}

fn parse_effect_speed(s: &str) -> Result<f32, String> {
    let val: f32 = s.parse().map_err(|e| format!("invalid float: {e}"))?;
    if (0.0..=10.0).contains(&val) {
        Ok(val)
    } else {
        Err(format!("value {val} not in range 0.0..=10.0"))
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "flyline",
    styles = get_styles(),
    after_help = "Read more at https://github.com/HalFrgrd/flyline",
)]
struct FlylineArgs {
    /// Show version information
    #[arg(long)]
    version: bool,
    /// Load Zsh history in addition to Bash history. Optionally specify a PATH to the Zsh history file
    #[arg(long = "load-zsh-history", value_name = "PATH", default_missing_value = "", num_args = 0..=1)]
    load_zsh_history: Option<String>,
    /// Show animations
    #[arg(long = "show-animations", default_missing_value = "true", num_args = 0..=1)]
    show_animations: Option<bool>,
    /// Run matrix animation in the terminal background. Use `on` to always show it, `off` to
    /// disable it, or an integer number of seconds to show it after that many seconds of
    /// inactivity (no keypress or mouse event). Defaults to `off`; passing the flag without a
    /// value is equivalent to `on`.
    #[arg(long = "matrix-animation", default_missing_value = "on", num_args = 0..=1, value_parser = parse_matrix_animation)]
    matrix_animation: Option<settings::MatrixAnimation>,
    /// Render frame rate in frames per second (1–120, default 24)
    #[arg(long = "frame-rate", value_name = "FPS", value_parser = clap::value_parser!(u8).range(1..=120))]
    frame_rate: Option<u8>,
    /// Mouse capture mode (disabled, simple, smart). Default is smart.
    #[arg(long = "mouse-mode", value_name = "MODE")]
    mouse_mode: Option<settings::MouseMode>,
    /// Send shell integration escape codes (OSC 133 / OSC 633): none, only-prompt-pos, or full
    #[arg(long = "send-shell-integration-codes", default_missing_value = "only-prompt-pos", num_args = 0..=1)]
    send_shell_integration_codes: Option<settings::ShellIntegrationLevel>,
    /// Whether to request the use of extended (kitty-protocol) keyboard codes during startup.
    /// Enabled by default; pass `--enable-extended-key-codes false` to
    /// disable it on terminals that misbehave when the request is sent.
    #[arg(long = "enable-extended-key-codes", default_missing_value = "true", num_args = 0..=1)]
    enable_extended_key_codes: Option<bool>,
    // Only for integration tests
    #[cfg(feature = "integration-tests")]
    #[arg(long = "run-tab-completion-tests")]
    run_tab_completion_tests: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

pub fn complete_flyline_args(
    raw_command: &str,
    cursor_byte: usize,
) -> anyhow::Result<Vec<clap_complete::CompletionCandidate>> {
    let current_dir = std::env::current_dir().ok();
    let current_dir_asdf = current_dir.as_ref().map(|p| p.to_path_buf());

    let mut command = FlylineArgs::command();

    let mut parser = dparser::DParser::from(raw_command);
    parser.walk_to_end();
    let tokens = parser
        .into_tokens()
        .into_iter()
        .map(|annoted_token| annoted_token.token)
        .collect::<Vec<_>>();

    // if the cursor is not in any of the tokens, append a whitespace token at the end so that we can generate completions for an empty token after the last word
    let tokens =
        if tokens.is_empty() || !tokens.iter().any(|t| t.byte_range().contains(&cursor_byte)) {
            let mut tokens = tokens;
            tokens.push(flash::lexer::Token {
                kind: flash::lexer::TokenKind::Whitespace(" ".to_string()),
                value: " ".to_string(),
                position: flash::lexer::Position {
                    line: 0,
                    column: 0,
                    byte: cursor_byte,
                },
            });
            tokens
        } else {
            tokens
        };

    let non_whitespace_tokens = tokens
        .into_iter()
        .filter(|x| !x.kind.is_whitespace() || x.byte_range().contains(&cursor_byte))
        .collect::<Vec<_>>();

    let index = non_whitespace_tokens
        .iter()
        .filter(|x| !x.kind.is_whitespace())
        .take_while(|&tok| tok.byte_range().end < cursor_byte)
        .count();

    let args_os_string = non_whitespace_tokens
        .iter()
        .map(|t| {
            if t.kind.is_whitespace() {
                std::ffi::OsString::new()
            } else {
                std::ffi::OsString::from(&t.value)
            }
        })
        .collect::<Vec<_>>();

    log::info!(
        "Completing command: cursor_byte: {}, parsed args: {:?}, index: {}, ",
        cursor_byte,
        args_os_string,
        index
    );

    match clap_complete::engine::complete(
        &mut command,
        args_os_string,
        index,
        current_dir_asdf.as_deref(),
    ) {
        Ok(candidates) => {
            log::info!("{:#?}", candidates);
            return Ok(candidates);
        }
        Err(e) => {
            log::error!("Error generating bash completion: {e}");
            return Err(anyhow::anyhow!("Error generating bash completion: {e}"));
        }
    };
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configure AI agent mode.
    ///
    /// When Alt+Enter is pressed, flyline invokes COMMAND with the current buffer
    /// (optionally prepended by SYSTEM_PROMPT) as the final argument.
    ///
    /// When --trigger-prefix is set, pressing Enter also activates agent mode
    /// if the buffer starts with the given prefix (the prefix is stripped before
    /// the buffer is sent to the command).
    ///
    /// Examples:
    ///   flyline set-agent-mode \
    ///     --system-prompt "Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \
    ///     --command 'copilot --reasoning-effort low --prompt'
    ///   flyline set-agent-mode --trigger-prefix ": " --command 'copilot --reasoning-effort low --prompt'
    ///
    /// See https://github.com/HalFrgrd/flyline/blob/master/examples/agent_mode.sh for more details and example usage.
    #[command(name = "set-agent-mode", verbatim_doc_comment)]
    AgentMode {
        /// Optional system prompt prepended to the buffer.
        /// The subprocess receives "<system-prompt>\n<buffer>" as its final argument.
        #[arg(long = "system-prompt")]
        system_prompt: Option<String>,
        /// Optional trigger prefix. When set, pressing Enter with a buffer that
        /// starts with this prefix activates agent mode (the prefix is stripped).
        #[arg(long = "trigger-prefix")]
        trigger_prefix: Option<String>,
        /// Command string to invoke; include any flags in the same string, e.g.
        /// --command 'copilot --reasoning-effort low --prompt'.
        /// The current buffer is appended as the final argument when Alt+Enter is pressed.
        #[arg(long = "command", required = true)]
        command: String,
    },
    /// Create a custom prompt widget.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with the widget output on every render.
    ///
    /// Widget types:
    ///   animation   Cycles through a list of frames at a given fps.
    ///   mouse-mode  Shows different text depending on whether mouse capture is enabled.
    ///   copy-buffer Shows clickable text that copies the current buffer to the clipboard.
    ///   custom      Runs a shell command and displays its output.
    ///
    /// Examples:
    ///   flyline create-prompt-widget animation --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
    ///   flyline create-prompt-widget animation --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'
    ///   flyline create-prompt-widget mouse-mode --name FLYLINE_MOUSE_MODE 'mouse is enabled' 'mouse is disabled'
    ///   flyline create-prompt-widget copy-buffer --name COPY_BUFFER '[copy]'
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_something.sh' --placeholder 10
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_something.sh' --block
    #[command(name = "create-prompt-widget", verbatim_doc_comment)]
    CreatePromptWidget {
        #[command(subcommand)]
        subcommand: PromptWidgetSubcommands,
    },
    /// Configure the colour palette.
    ///
    /// Style strings follow rich's syntax: a space-separated list of attributes
    /// (bold, dim, italic, underline, blink, reverse, strike) and colours
    /// (e.g. red, #ff0000, rgb(255,0,0), color(196)).
    ///
    /// Valid style names:
    ///   recognised-command, unrecognised-command, single-quoted-text,
    ///   double-quoted-text, secondary-text, inline-suggestion, tutorial-hint,
    ///   matching-char, opening-and-closing-pair, normal-text, comment,
    ///   env-var, markdown-heading1, markdown-heading2, markdown-heading3,
    ///   markdown-code, key-sequence-style, selected-text
    ///
    /// Examples:
    ///   flyline set-style --default-theme dark
    ///   flyline set-style inline-suggestion="dim italic"
    ///   flyline set-style matching-char="bold green"
    ///   flyline set-style --default-theme light matching-char="bold blue"
    ///   flyline set-style recognised-command="green" unrecognised-command="bold red"
    ///   flyline set-style secondary-text="dim" tutorial-hint="bold italic"
    #[command(name = "set-style", verbatim_doc_comment)]
    SetColour {
        /// Apply a built-in colour preset for dark or light terminals.
        #[arg(long = "default-theme", value_name = "MODE")]
        default_theme: Option<settings::ColourTheme>,
        /// One or more palette style assignments as NAME=STYLE.
        /// NAME is the kebab-case style slot name; STYLE is a rich-style string.
        #[arg(value_name = "NAME=STYLE")]
        styles: Vec<String>,
    },
    /// Configure the cursor appearance and animation.
    ///
    /// Controls which backend renders the cursor, how it moves (interpolation),
    /// what it looks like (style), and any blinking/fading effect.
    ///
    /// Style strings follow rich's syntax: a space-separated list of colours
    /// and attributes.  For cursor styles a single colour (e.g. `red`) is
    /// interpreted as the **background** colour of the cursor cell.
    /// Use `"pink on white"` for an explicit foreground and background.
    /// The special value `"reverse"` inverts the colours of the cell under
    /// the cursor.
    ///
    /// Examples:
    ///   flyline set-cursor --backend flyline
    ///   flyline set-cursor --style "reverse"
    ///   flyline set-cursor --style "red"
    ///   flyline set-cursor --style "pink on white"
    ///   flyline set-cursor --interpolate 16 --interpolate-easing out-cubic
    ///   flyline set-cursor --effect blink --effect-speed 2.0
    ///   flyline set-cursor --effect fade --effect-easing in-out-sine
    ///   flyline set-cursor --interpolate none
    #[command(name = "set-cursor", verbatim_doc_comment)]
    SetCursor {
        /// Cursor rendering backend.  `flyline` renders a custom cursor (the default);
        /// `terminal` defers to the terminal emulator.
        #[arg(long)]
        backend: Option<cursor::CursorBackend>,
        /// Interpolation speed (1/second), or `none` to disable
        /// interpolation.  Default is `16`.
        #[arg(long, value_name = "SPEED|none")]
        interpolate: Option<String>,
        /// Easing function for position interpolation.  Default is `linear`.
        #[arg(long, value_name = "EASING")]
        interpolate_easing: Option<cursor::CursorEasing>,
        /// Cursor style.  A single colour (e.g. `red`) is the cursor background.
        /// `"pink on white"` sets foreground and background.  `"reverse"` inverts
        /// the cell colours.  Default is a white block modulated by the effect.
        #[arg(long, value_name = "STYLE")]
        style: Option<String>,
        /// Visual effect applied to the cursor: `fade`, `blink`, or `none`.
        #[arg(long)]
        effect: Option<cursor::CursorEffect>,
        /// Speed multiplier for the cursor effect (default is `1.0`).
        #[arg(long, value_name = "SPEED", value_parser = parse_effect_speed)]
        effect_speed: Option<f32>,
        /// Easing function for the cursor effect intensity.  Default is `linear`.
        #[arg(long, value_name = "EASING")]
        effect_easing: Option<cursor::CursorEasing>,
    },
    /// Manage keybindings.
    ///
    /// Use 'flyline key bind <KEY> <CONTEXT_EXPR>=<ACTION>' to bind a key sequence to an action.
    /// Use 'flyline key list' to view all current bindings.
    /// Use 'flyline key remap <FROM> <TO>' to translate one key or modifier to another before
    /// bindings are matched.
    ///
    /// KEY is a combination like "Ctrl+Enter", "Alt+Left", or "F1".
    /// Modifiers: Ctrl (Control), Shift, Alt (Option), Meta,
    ///   Super (Cmd, Command, Gui, Win), Hyper.
    /// Keys: Enter (Ret, Return), Backspace (Bkspc, Bs), Tab, BackTab, Esc (Escape),
    ///   Space (Spc), Delete (Del), Insert (Ins), Left, Right, Up, Down, Home, End,
    ///   PageUp (PgUp), PageDown (PgDown, PgDn), Null,
    ///   CapsLock (Caps, Caps_Lock), ScrollLock (Scroll_Lock), NumLock (Num_Lock),
    ///   PrintScreen (PrtScn, Print_Screen), Pause, Menu, KeypadBegin (Keypad_Begin),
    ///   F1-F255, Media:<name> (e.g. Media:Play, Media:Pause, Media:Stop,
    ///   Media:FastForward, Media:Rewind, Media:TrackNext, Media:TrackPrevious,
    ///   Media:RaiseVolume, Media:LowerVolume, Media:Mute),
    ///   Modifier:<name> (e.g. Modifier:LeftShift, Modifier:RightCtrl,
    ///   Modifier:LeftAlt, Modifier:LeftSuper).
    ///
    /// Tab completion is available: type 'flyline key bind <KEY> <Tab>' to browse
    /// all available context variables and actions interactively.
    ///
    /// Examples:
    ///   flyline key bind Ctrl+Enter always=submitOrNewline
    ///   flyline key list
    #[command(name = "key", verbatim_doc_comment)]
    Key {
        /// Show the last key event and dispatched action above the prompt.
        #[arg(long = "debug", default_missing_value = "true", num_args = 0..=1)]
        debug: Option<bool>,
        #[command(subcommand)]
        subcommand: Option<KeySubcommands>,
    },
    /// Logging commands: dump, configure level, or stream logs.
    ///
    /// Examples:
    ///   flyline log dump
    ///   flyline log set-level debug
    ///   flyline log stream /tmp/flyline.log
    ///   flyline log stream terminal
    #[command(name = "log", verbatim_doc_comment)]
    Log {
        #[command(subcommand)]
        subcommand: LogSubcommands,
    },
    /// Run the interactive tutorial for first-time users.
    ///
    /// Pass `false` to disable the tutorial.
    ///
    /// Examples:
    ///   flyline run-tutorial
    ///   flyline run-tutorial false
    #[command(name = "run-tutorial", verbatim_doc_comment)]
    RunTutorial {
        /// Enable or disable the tutorial. Defaults to `true`.
        #[arg(default_missing_value = "true", num_args = 0..=1)]
        enabled: Option<bool>,
    },
    /// Configure the inline editor.
    ///
    /// Controls behaviours of the buffer editor: automatic closing of bracket
    /// pairs and quotes, inline history suggestions, and whether mouse clicks
    /// and drags change the buffer cursor and selection.
    ///
    /// Examples:
    ///   flyline editor --auto-close-chars false
    ///   flyline editor --show-inline-history false
    ///   flyline editor --select-with-mouse false
    ///   flyline editor --auto-close-chars true --select-with-mouse true
    #[command(name = "editor", verbatim_doc_comment)]
    Editor {
        /// Enable automatic closing character insertion (e.g. insert `)` after `(`).
        #[arg(long = "auto-close-chars", default_missing_value = "true", num_args = 0..=1)]
        auto_close_chars: Option<bool>,
        /// Show inline history suggestions.
        #[arg(long = "show-inline-history", default_missing_value = "true", num_args = 0..=1)]
        show_inline_history: Option<bool>,
        /// Whether mouse clicks and drags on the command buffer change the
        /// cursor position and selection. Default is `true`. When `false`,
        /// mouse interaction with the buffer does not change the selection.
        #[arg(long = "select-with-mouse", default_missing_value = "true", num_args = 0..=1)]
        select_with_mouse: Option<bool>,
    },
    /// Run a command with --help, parse the output, and print a Bash completion
    /// script to stdout.
    ///
    /// Pass the name or path of the command to synthesise Bash tab-completions
    /// for that tool on the fly.  Subcommands are automatically discovered by
    /// running `COMMAND SUBCOMMAND --help` for each subcommand listed in the
    /// top-level help output.
    ///
    /// Examples:
    ///   flyline comp-spec-synthesis git
    ///   flyline comp-spec-synthesis /usr/local/bin/mytool
    #[command(name = "comp-spec-synthesis", verbatim_doc_comment)]
    CompSpecSynthesis {
        /// Name or path of the command to synthesise completions for.
        command: String,
    },
}

#[derive(Subcommand, Debug)]
enum KeySubcommands {
    /// Bind a key sequence to an action, optionally guarded by a context expression.
    ///
    /// KEY_SEQUENCE is a key combination such as "Ctrl+Enter" or "Alt+Left".
    /// CONTEXT_AND_ACTION has the form `<contextExpr>=<actionName>`, where
    /// `<contextExpr>` is an `&&`-separated chain of camelCase context variables
    /// (each optionally prefixed with `!` to negate).  Use `always` for
    /// unconditional bindings.  Parentheses and `||` are not supported.
    ///
    /// Available context variables: always, bufferIsEmpty, fuzzyHistorySearch,
    ///   tabCompletionWaiting, tabCompletion, tabCompletionAvailable,
    ///   tabCompletionMultiColAvailable, tabCompletionsNoFilteredResults, tabCompletionsNoResults,
    ///   agentModeWaiting, agentOutputSelection, agentError, inlineSuggestionAvailable,
    ///   cursorAtEnd, cursorAtStart, promptDirSelect, textSelected, multilineBuffer.
    ///
    /// Examples:
    ///   flyline key bind Ctrl+Enter always=submitOrNewline
    ///   flyline key bind Tab inlineSuggestionAvailable&&cursorAtEnd=acceptInlineSuggestion
    ///   flyline key bind Alt+Left always=moveLeftOneWordFineGrained
    #[command(name = "bind", verbatim_doc_comment, disable_help_flag = true)]
    Bind {
        /// Key sequence to bind (e.g. "Ctrl+Enter", "Alt+Left").
        #[arg(num_args = 1, hide = true, add = ArgValueCompleter::new(actions::key_sequence_completer))]
        key_sequence: String,
        /// Context expression and action in the form `<contextExpr>=<actionName>`
        /// (e.g. "always=submitOrNewline").
        #[arg(add = ArgValueCompleter::new(actions::possible_context_action_completions), num_args = 1)]
        context_and_action: String,
    },
    /// List all keybindings from lowest to highest priority.
    ///
    /// User-defined bindings are marked with * in the User column and have
    /// higher priority than the built-in defaults.
    ///
    /// Optionally supply a KEY_SEQUENCE (e.g. "Tab", "Ctrl+r") to show only
    /// bindings that the given key would trigger.
    #[command(name = "list")]
    List {
        /// Optional key sequence to filter by (e.g. "Tab", "Ctrl+r").
        key_sequence: Option<String>,
    },
    /// Remap a key or modifier to another key or modifier.
    ///
    /// When a key event arrives, FROM is translated to TO before being matched
    /// against bindings.  Keys can only be remapped to keys; modifiers can only
    /// be remapped to modifiers.
    ///
    /// Examples:
    ///   flyline key remap tab z       # pressing Tab acts like pressing z
    ///   flyline key remap alt ctrl    # pressing Alt acts like pressing Ctrl
    ///   flyline key remap ctrl alt    # combined with above: swap Ctrl and Alt
    #[command(name = "remap", verbatim_doc_comment)]
    Remap {
        /// The key or modifier to remap from (e.g. "tab", "alt").
        from: String,
        /// The key or modifier to remap to (e.g. "z", "ctrl").
        to: String,
    },
}

#[derive(Subcommand, Debug)]
enum LogSubcommands {
    /// Dump all in-memory log entries to stdout.
    ///
    /// Examples:
    ///   flyline log dump
    #[command(name = "dump", verbatim_doc_comment)]
    Dump,
    /// Set the logging level.
    ///
    /// LEVEL is one of: error, warn, info, debug, trace
    ///
    /// Examples:
    ///   flyline log set-level debug
    ///   flyline log set-level trace
    #[command(name = "set-level", verbatim_doc_comment)]
    SetLevel {
        /// Logging level to apply.
        #[arg(value_name = "LEVEL")]
        level: LogLevelArg,
    },
    /// Stream logs to a file path or to the terminal.
    ///
    /// Use `terminal` to display the last 20 log lines inside the flyline TUI
    /// on every render.  Otherwise supply a file path; existing log entries
    /// are written to the file and subsequent entries are appended.
    /// Use `stderr` to stream to standard error.
    ///
    /// Examples:
    ///   flyline log stream /tmp/flyline.log
    ///   flyline log stream stderr
    ///   flyline log stream terminal
    #[command(name = "stream", verbatim_doc_comment)]
    Stream {
        /// Destination: a file path, `stderr`, or `terminal`.
        #[arg(value_name = "FILEPATH|terminal")]
        dest: String,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum LogLevelArg {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevelArg> for log::LevelFilter {
    fn from(level: LogLevelArg) -> Self {
        match level {
            LogLevelArg::Error => log::LevelFilter::Error,
            LogLevelArg::Warn => log::LevelFilter::Warn,
            LogLevelArg::Info => log::LevelFilter::Info,
            LogLevelArg::Debug => log::LevelFilter::Debug,
            LogLevelArg::Trace => log::LevelFilter::Trace,
        }
    }
}

#[derive(Subcommand, Debug)]
enum PromptWidgetSubcommands {
    /// Create a custom prompt animation that cycles through frames.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with the current animation frame on every render.  Frames may include
    /// ANSI colour sequences written as `\e` (e.g. `\e[33m`).
    ///
    /// Examples:
    ///   flyline create-prompt-widget animation --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
    ///   flyline create-prompt-widget animation --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'
    ///
    /// See https://github.com/HalFrgrd/flyline/blob/master/examples/animations.sh for more details and example usage.
    #[command(name = "animation", verbatim_doc_comment)]
    Animation {
        /// Name to embed in prompt strings as the animation placeholder.
        #[arg(long)]
        name: String,
        /// Playback speed in frames per second (default: 10).
        #[arg(long, default_value = "10")]
        fps: f64,
        /// Reverse direction at each end instead of wrapping (ping-pong / bounce mode).
        #[arg(long)]
        ping_pong: bool,
        /// One or more animation frames (positional).  Use `\e` for the ESC character.
        frames: Vec<String>,
    },
    /// Show different text depending on whether mouse capture is enabled.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with ENABLED_TEXT when mouse capture is on, and DISABLED_TEXT when off.
    ///
    /// Examples:
    ///   flyline create-prompt-widget mouse-mode '🖱️' '🔴'
    ///   # Now use FLYLINE_MOUSE_MODE in your prompt:
    ///   PS1='\u@\h:\w [FLYLINE_MOUSE_MODE] $ '
    ///
    ///   flyline create-prompt-widget mouse-mode --name MOUSE_MODE "on " "off"
    #[command(name = "mouse-mode", verbatim_doc_comment)]
    MouseMode {
        /// Name to embed in prompt strings as the widget placeholder.
        /// Defaults to `FLYLINE_MOUSE_MODE`.
        #[arg(long, default_value = "FLYLINE_MOUSE_MODE")]
        name: String,
        /// Text to display when mouse capture is enabled.
        enabled_text: String,
        /// Text to display when mouse capture is disabled.
        disabled_text: String,
    },
    /// Show clickable text that copies the current command buffer to the clipboard.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with TEXT. Clicking the rendered widget copies the current command buffer
    /// to the clipboard via OSC 52.
    ///
    /// Examples:
    ///   flyline create-prompt-widget copy-buffer '[copy]'
    ///   # Now use FLYLINE_COPY_BUFFER in your prompt:
    ///   RPS1=' FLYLINE_COPY_BUFFER'
    #[command(name = "copy-buffer", verbatim_doc_comment)]
    CopyBuffer {
        /// Name to embed in prompt strings as the widget placeholder.
        /// Defaults to `FLYLINE_COPY_BUFFER`.
        #[arg(long, default_value = "FLYLINE_COPY_BUFFER")]
        name: String,
        /// Text to display in the prompt.
        text: String,
    },
    /// Run a shell command and display its output in the prompt.
    ///
    /// The output is passed through Bash's decode_prompt_string so Bash prompt
    /// escape sequences (e.g. \u, \w, ANSI colour codes) are fully supported.
    ///
    /// Examples:
    ///   # Non-blocking (default): runs in the background; shows the previous output
    ///   # while the command is running (empty on the first render).
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_slow_git_metrics.sh'
    ///   # PS1 usage:
    ///   PS1='\u@\h:\w [CUSTOM_WIDGET1] $ '
    ///
    ///   # Non-blocking with a 10-space placeholder while the new output is being computed.
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_slow_git_metrics.sh' --placeholder 10
    ///
    ///   # Blocking: waits for the command to finish before showing the prompt.
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET2 --command 'run_something.sh' --block
    ///
    ///   # Blocking with a 500 ms timeout; falls back to placeholder if slower.
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET3 --command 'run_slow.sh --flag' --block 500 --placeholder prev
    #[command(name = "custom", verbatim_doc_comment)]
    Custom {
        /// Name to embed in prompt strings as the widget placeholder.
        #[arg(long)]
        name: String,
        /// Command string to run; include any flags in the same string, e.g.
        /// --command './widget.sh --someflag'.
        #[arg(long)]
        command: String,
        /// Block until the command finishes, optionally with a timeout in milliseconds.
        /// With no value, polls indefinitely (i32::MAX ms ≈ 24.8 days).  If the
        /// timeout expires the command continues running in the background and
        /// subsequent renders will pick up its output.
        // default_missing_value "2147483647" == i32::MAX; proc-macro attributes
        // require a string literal so the constant cannot be referenced directly.
        #[arg(long, num_args = 0..=1, default_missing_value = "2147483647", value_name = "MS")]
        block: Option<i32>,
        /// What to show while the command is running.  Either a number (spaces) or
        /// 'prev' (use the previous output of the command).
        #[arg(long)]
        placeholder: Option<String>,
    },
}

// Global state for our custom input stream
static FLYLINE_INSTANCE_PTR: Mutex<Option<Box<Flyline>>> = Mutex::new(None);

fn catch_unwind_safe<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

fn report_stderr_no_panic(message: &str) {
    let _ = catch_unwind_safe(|| {
        eprintln!("{message}");
    });
}

fn report_error_no_panic(message: &str) {
    let _ = catch_unwind_safe(|| {
        log::error!("{message}");
    });
}

// C-compatible getter function that bash will call
extern "C" fn flyline_get_char() -> c_int {
    if let Some(boxed) = FLYLINE_INSTANCE_PTR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
    {
        match catch_unwind_safe(|| boxed.get()) {
            Ok(c) => c,
            Err(_) => {
                // writing to stderr can panic if master pty side has been closed.
                report_stderr_no_panic(
                    "flyline: app panicked; recovering with EOF. Please create an issue with the steps to reproduce at https://github.com/HalFrgrd/flyline/issues.",
                );
                report_error_no_panic("app panicked; recovering with EOF");

                std::thread::sleep(std::time::Duration::from_millis(1000));
                bash_symbols::EOF
            }
        }
    } else {
        report_stderr_no_panic("flyline_get_char: FLYLINE_INSTANCE_PTR is None");
        bash_symbols::EOF
    }
}

// C-compatible ungetter function that bash will call
extern "C" fn flyline_unget_char(c: c_int) -> c_int {
    if let Some(boxed) = FLYLINE_INSTANCE_PTR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
    {
        return match catch_unwind_safe(|| boxed.unget(c)) {
            Ok(unget_char) => unget_char,
            Err(_) => {
                report_stderr_no_panic("flyline: unget handler panicked; ignoring.");
                report_error_no_panic("flyline_unget_char panicked; returning original character");
                c
            }
        };
    }
    report_stderr_no_panic("flyline_unget_char: FLYLINE_INSTANCE_PTR is None");
    c
}

extern "C" fn flyline_call_command(words: *const bash_symbols::WordList) -> c_int {
    let result = catch_unwind_safe(|| {
        if let Some(boxed) = FLYLINE_INSTANCE_PTR
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            return boxed.call(words);
        }
        report_stderr_no_panic("flyline_call_command: FLYLINE_INSTANCE_PTR is None");
        0
    });
    match result {
        Ok(code) => code,
        Err(_) => {
            report_stderr_no_panic("flyline: command handler panicked; ignoring.");
            report_error_no_panic("flyline_call_command panicked; returning failure");
            bash_symbols::BuiltinExitCode::Usage as c_int
        }
    }
}

#[derive(Debug)]
struct Flyline {
    content: Vec<u8>,
    position: usize,
    settings: settings::Settings,
}

impl Flyline {
    fn new() -> Self {
        Self {
            content: vec![],
            position: 0,
            settings: settings::Settings::default(),
        }
    }

    fn call(&mut self, words: *const bash_symbols::WordList) -> c_int {
        let mut args = vec![];
        unsafe {
            let mut current = words;
            while !current.is_null() {
                let word_desc = &*(*current).word;
                if !word_desc.word.is_null() {
                    let c_str = std::ffi::CStr::from_ptr(word_desc.word);
                    if let Ok(str_slice) = c_str.to_str() {
                        args.push(str_slice);
                        // TODO what do the flags mean?
                        // println!("arg: {} flags: {}", str_slice, word_desc.flags);
                    }
                }
                current = (*current).next;
            }
        }
        log::debug!("flyline called with args: {:?}", args);

        // args contains words from WordList; first word is not the command name unlike argv
        let args_with_prog = std::iter::once("flyline").chain(args.iter().copied());
        match FlylineArgs::try_parse_from(args_with_prog) {
            Ok(parsed) if !args.is_empty() => {
                log::debug!("Parsed flyline arguments: {:?}", parsed);

                if parsed.version {
                    println!(
                        "flyline version {} ({}) git:{} built:{}",
                        env!("CARGO_PKG_VERSION"),
                        if cfg!(debug_assertions) {
                            "debug"
                        } else {
                            "release"
                        },
                        env!("GIT_HASH"),
                        env!("BUILD_TIME"),
                    );
                }

                if let Some(path) = parsed.load_zsh_history {
                    self.settings.zsh_history_path = Some(path);
                }

                if let Some(enabled) = parsed.show_animations {
                    log::info!("Animations disabled: {}", enabled);
                    self.settings.show_animations = enabled;
                }

                if let Some(val) = parsed.matrix_animation {
                    log::info!("Matrix animation set to {:?}", val);
                    self.settings.matrix_animation = val;
                }

                if let Some(fps) = parsed.frame_rate {
                    log::info!("Frame rate set to {}", fps);
                    self.settings.frame_rate = fps;
                }

                if let Some(mode) = parsed.mouse_mode {
                    log::info!("Mouse mode set to {:?}", mode);
                    self.settings.mouse_mode = mode;
                }

                if let Some(level) = parsed.send_shell_integration_codes {
                    log::info!("Shell integration codes set to {:?}", level);
                    self.settings.send_shell_integration_codes = level;
                }

                if let Some(enabled) = parsed.enable_extended_key_codes {
                    log::info!("Extended keyboard codes enabled: {}", enabled);
                    self.settings.enable_extended_key_codes = enabled;
                }

                match parsed.command {
                    Some(Commands::AgentMode {
                        system_prompt,
                        trigger_prefix,
                        command,
                    }) => {
                        let command_args: Vec<String> =
                            shlex::split(&command).unwrap_or_else(|| {
                                command.split_whitespace().map(String::from).collect()
                            });
                        if command_args.is_empty() {
                            eprintln!("flyline set-agent-mode: --command must not be empty");
                            return bash_symbols::BuiltinExitCode::Usage as c_int;
                        }
                        log::info!(
                            "AI command set: {:?} (trigger_prefix={:?})",
                            command_args,
                            trigger_prefix
                        );
                        self.settings.agent_commands.insert(
                            trigger_prefix.clone(),
                            settings::AgentModeCommand {
                                command: command_args,
                                system_prompt: system_prompt.clone(),
                            },
                        );
                    }
                    Some(Commands::CreatePromptWidget { subcommand }) => match subcommand {
                        PromptWidgetSubcommands::Animation {
                            name,
                            fps,
                            frames,
                            ping_pong,
                        } => {
                            if fps <= 0.0 {
                                eprintln!(
                                    "flyline create-prompt-widget animation: --fps must be greater than 0 (got {}); animation '{}' not registered",
                                    fps, name
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            log::info!(
                                "Registering animation '{}' at {} fps with {} frame(s) (ping_pong={})",
                                name,
                                fps,
                                frames.len(),
                                ping_pong
                            );
                            self.settings.custom_animations.insert(
                                name.clone(),
                                settings::PromptAnimation {
                                    name,
                                    fps,
                                    frames,
                                    ping_pong,
                                },
                            );
                        }
                        PromptWidgetSubcommands::MouseMode {
                            name,
                            enabled_text,
                            disabled_text,
                        } => {
                            log::info!(
                                "Registering mouse-mode widget '{}' (enabled={:?}, disabled={:?})",
                                name,
                                enabled_text,
                                disabled_text
                            );
                            self.settings.custom_prompt_widgets.insert(
                                name.clone(),
                                settings::PromptWidget::MouseMode(
                                    settings::PromptWidgetMouseMode {
                                        name,
                                        enabled_text,
                                        disabled_text,
                                    },
                                ),
                            );
                        }
                        PromptWidgetSubcommands::CopyBuffer { name, text } => {
                            log::info!(
                                "Registering copy-buffer widget '{}' (text={:?})",
                                name,
                                text
                            );
                            self.settings.custom_prompt_widgets.insert(
                                name.clone(),
                                settings::PromptWidget::CopyBuffer(
                                    settings::PromptWidgetCopyBuffer { name, text },
                                ),
                            );
                        }
                        PromptWidgetSubcommands::Custom {
                            name,
                            command,
                            block,
                            placeholder,
                        } => {
                            let command_args: Vec<String> =
                                shlex::split(&command).unwrap_or_else(|| {
                                    command.split_whitespace().map(String::from).collect()
                                });
                            if command_args.is_empty() {
                                eprintln!(
                                    "flyline create-prompt-widget custom: --command must not be empty"
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            if let Some(ms) = block {
                                if ms < 0 {
                                    eprintln!(
                                        "flyline create-prompt-widget custom: --block timeout must be non-negative (got {})",
                                        ms
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            }
                            let placeholder_spec = match placeholder {
                                None => None,
                                Some(ref s) if s == "prev" => Some(settings::Placeholder::Prev),
                                Some(ref s) => match s.parse::<usize>() {
                                    Ok(n) => Some(settings::Placeholder::Spaces(n)),
                                    Err(_) => {
                                        eprintln!(
                                            "flyline create-prompt-widget custom: --placeholder must be a number or 'prev', got {:?}",
                                            s
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                },
                            };
                            log::info!(
                                "Registering custom widget '{}' (command={:?}, block={:?}, placeholder={:?})",
                                name,
                                command_args,
                                block,
                                placeholder
                            );
                            self.settings.custom_prompt_widgets.insert(
                                name.clone(),
                                settings::PromptWidget::Custom(settings::PromptWidgetCustom {
                                    name,
                                    command: command_args,
                                    block,
                                    placeholder: placeholder_spec.unwrap_or_default(),
                                    prev_output: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
                                }),
                            );
                        }
                    },
                    Some(Commands::SetColour {
                        default_theme,
                        styles,
                    }) => {
                        if let Some(preset) = default_theme {
                            self.settings.colour_palette.apply_theme(preset);
                            log::info!("Colour theme set to {:?}", preset);
                        }

                        for spec in &styles {
                            let (name, style_str) = match spec.split_once('=') {
                                Some(pair) => pair,
                                None => {
                                    eprintln!(
                                        "flyline set-style: argument must be NAME=STYLE, got {:?}",
                                        spec
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            };
                            let kind = match name.parse::<palette::PaletteStyleKind>() {
                                Ok(k) => k,
                                Err(_) => {
                                    eprintln!(
                                        "flyline set-style: unknown style name {:?}. Run 'flyline set-style --help' for valid names.",
                                        name
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            };
                            match palette::parse_str_to_style(style_str) {
                                Ok(style) => {
                                    self.settings.colour_palette.set(kind, style);
                                    log::info!("{} style set to {:?}", name, style_str);
                                }
                                Err(e) => {
                                    eprintln!(
                                        "flyline set-style: invalid style for {:?}: {}",
                                        name, e
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            }
                        }
                    }
                    Some(Commands::Key { debug, subcommand }) => {
                        if let Some(enabled) = debug {
                            log::info!("Key debug mode enabled: {}", enabled);
                            self.settings.key_debug = enabled;
                        }

                        match subcommand {
                            Some(KeySubcommands::Bind {
                                key_sequence,
                                context_and_action,
                            }) => {
                                let binding = actions::Binding::try_new_from_strs(
                                    &key_sequence,
                                    &context_and_action,
                                );
                                match binding {
                                    Ok(binding) => {
                                        log::info!(
                                            "Registering key binding: {} -> {}",
                                            key_sequence,
                                            context_and_action
                                        );
                                        self.settings.keybindings.push(binding);
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "flyline key bind: failed to parse key sequence '{}' or context/action '{}': {}",
                                            key_sequence, context_and_action, e
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                            Some(KeySubcommands::List { key_sequence }) => {
                                actions::print_bindings_table(
                                    &self.settings.keybindings,
                                    key_sequence.as_deref(),
                                    &self.settings.key_remappings,
                                );
                            }
                            Some(KeySubcommands::Remap { from, to }) => {
                                match actions::try_parse_remap(&from, &to) {
                                    Ok(remap) => {
                                        log::info!("Registering key remap: {} -> {}", from, to);
                                        self.settings.key_remappings.push(remap);
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "flyline key remap: failed to parse remap '{}' -> '{}': {}",
                                            from, to, e
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                    None => {}
                    Some(Commands::Log { subcommand }) => match subcommand {
                        LogSubcommands::Dump => {
                            if let Err(e) = logging::dump_logs_stdout() {
                                eprintln!("Failed to dump logs: {}", e);
                            }
                        }
                        LogSubcommands::SetLevel { level } => {
                            let filter = log::LevelFilter::from(level);
                            log::set_max_level(filter);
                            log::info!("Log level set to {:?}", filter);
                        }
                        LogSubcommands::Stream { dest } => match logging::stream_logs(&dest) {
                            Ok(()) => {
                                if dest == "terminal" {
                                    log::info!("Log streaming to terminal");
                                } else {
                                    println!("Flyline logs streaming to {}", dest);
                                }
                            }
                            Err(e) => eprintln!("Failed to stream logs: {}", e),
                        },
                    },
                    Some(Commands::RunTutorial { enabled }) => {
                        let enabled = enabled.unwrap_or(true);
                        log::info!("Run tutorial set to {}", enabled);
                        self.settings.run_tutorial = enabled;
                        if enabled {
                            self.settings.tutorial_step = tutorial::TutorialStep::Welcome;
                            // clear the terminal:
                            if let Err(e) = crossterm::execute!(
                                std::io::stdout(),
                                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                                crossterm::cursor::MoveTo(0, 0)
                            ) {
                                log::warn!("Failed to clear terminal: {}", e);
                            }
                        } else {
                            self.settings.tutorial_step = tutorial::TutorialStep::NotRunning;
                        }
                    }
                    Some(Commands::Editor {
                        auto_close_chars,
                        show_inline_history,
                        select_with_mouse,
                    }) => {
                        if let Some(enabled) = auto_close_chars {
                            log::info!("Auto closing char set to {}", enabled);
                            self.settings.auto_close_chars = enabled;
                        }
                        if let Some(enabled) = show_inline_history {
                            log::info!("Inline history suggestions set to {}", enabled);
                            self.settings.show_inline_history = enabled;
                        }
                        if let Some(enabled) = select_with_mouse {
                            log::info!("Select with mouse set to {}", enabled);
                            self.settings.select_with_mouse = enabled;
                        }
                    }
                    Some(Commands::CompSpecSynthesis { command }) => {
                        match comp_spec_synthesis::synthesize_completion(&command, |args| {
                            let prev_sigchld =
                                unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };
                            let ret = comp_spec_synthesis::run_help(&command, args);
                            unsafe { libc::signal(libc::SIGCHLD, prev_sigchld) };
                            ret
                        }) {
                            Ok(parsed_cmd) => {
                                let cmd_name = std::path::Path::new(&command)
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(&command)
                                    .to_string();
                                let mut clap_cmd =
                                    comp_spec_synthesis::to_clap_command(&parsed_cmd);
                                let mut output = Vec::new();
                                generate(Shell::Bash, &mut clap_cmd, &cmd_name, &mut output);
                                match std::str::from_utf8(&output) {
                                    Ok(s) => print!("{}", s),
                                    Err(e) => {
                                        log::error!(
                                            "flyline comp-spec-synthesis: failed to encode output: {}",
                                            e
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("flyline comp-spec-synthesis: {}", e);
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                        }
                    }
                    Some(Commands::SetCursor {
                        backend,
                        interpolate,
                        interpolate_easing,
                        style,
                        effect,
                        effect_speed,
                        effect_easing,
                    }) => {
                        // set backend first since it affects the validity of other options
                        if let Some(b) = backend {
                            log::info!("Cursor backend set to {:?}", b);
                            self.settings.cursor_config.backend = b;
                            if b == cursor::CursorBackend::Terminal {
                                if style.is_some()
                                    || effect.is_some()
                                    || effect_speed.is_some()
                                    || effect_easing.is_some()
                                {
                                    eprintln!(
                                        "flyline set-cursor: --style, --effect, --effect-speed, and --effect-easing require --backend flyline"
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            }
                        }

                        if let Some(interp_str) = interpolate {
                            if interp_str.eq_ignore_ascii_case("none") {
                                log::info!("Cursor interpolation disabled");
                                self.settings.cursor_config.interpolate = None;
                            } else {
                                match interp_str.parse::<f32>() {
                                    Ok(speed) if speed > 0.0 => {
                                        log::info!("Cursor interpolation speed set to {}", speed);
                                        self.settings.cursor_config.interpolate = Some(speed);
                                    }
                                    _ => {
                                        eprintln!(
                                            "flyline set-cursor: --interpolate must be a positive number or 'none' (got {:?})",
                                            interp_str
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                        }

                        if let Some(easing) = interpolate_easing {
                            log::info!("Cursor interpolation easing set to {:?}", easing);
                            self.settings.cursor_config.interpolate_easing = easing;
                        }

                        if let Some(style_str) = style {
                            if self.settings.cursor_config.backend
                                == cursor::CursorBackend::Terminal
                            {
                                eprintln!("flyline set-cursor: --style requires --backend flyline");
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            match palette::parse_cursor_style_str(&style_str) {
                                Ok(s) => {
                                    log::info!("Cursor style set to {:?}", s);
                                    self.settings.cursor_config.style = s;
                                }
                                Err(e) => {
                                    eprintln!(
                                        "flyline set-cursor: invalid --style {:?}: {}",
                                        style_str, e
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            }
                        }

                        if let Some(eff) = effect {
                            if self.settings.cursor_config.backend
                                == cursor::CursorBackend::Terminal
                            {
                                eprintln!(
                                    "flyline set-cursor: --effect requires --backend flyline"
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            if eff == cursor::CursorEffect::Fade
                                && let CursorStyleConfig::Custom(style) =
                                    self.settings.cursor_config.style
                            {
                                match style.bg {
                                    Some(ratatui::style::Color::Rgb(..)) => {}
                                    _ => {
                                        eprintln!(
                                            "flyline set-cursor: --effect fade requires a custom style with an RGB background color (e.g. '#ff0000')",
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                            log::info!("Cursor effect set to {:?}", eff);
                            self.settings.cursor_config.effect = eff;
                        }

                        if let Some(speed) = effect_speed {
                            if self.settings.cursor_config.backend
                                == cursor::CursorBackend::Terminal
                            {
                                eprintln!(
                                    "flyline set-cursor: --effect-speed requires --backend flyline"
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            if speed > 0.0 {
                                log::info!("Cursor effect speed set to {}", speed);
                                self.settings.cursor_config.effect_speed = speed;
                            } else {
                                eprintln!(
                                    "flyline set-cursor: --effect-speed must be positive (got {})",
                                    speed
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                        }

                        if let Some(easing) = effect_easing {
                            if self.settings.cursor_config.backend
                                == cursor::CursorBackend::Terminal
                            {
                                eprintln!(
                                    "flyline set-cursor: --effect-easing requires --backend flyline"
                                );
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                            log::info!("Cursor effect easing set to {:?}", easing);
                            self.settings.cursor_config.effect_easing = easing;
                        }
                    }
                }

                #[cfg(feature = "integration-tests")]
                if parsed.run_tab_completion_tests {
                    self.settings.run_tab_completion_tests = true;
                    println!("Running tab completion tests...");
                    let prev_sigchld = unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };
                    app::get_command(&mut self.settings);
                    unsafe { libc::signal(libc::SIGCHLD, prev_sigchld) };
                    println!("Finished running tab completion tests.");
                }

                bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int
            }
            Ok(_) => {
                log::debug!("No arguments provided to flyline");
                FlylineArgs::command().print_help().ok();
                bash_symbols::BuiltinExitCode::Usage as c_int
            }
            Err(err) => {
                match err.kind() {
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                        // user asked for --help / --version
                        err.print().unwrap();
                        bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int
                    }
                    ErrorKind::UnknownArgument
                    | ErrorKind::InvalidValue
                    | ErrorKind::InvalidSubcommand
                    | ErrorKind::MissingRequiredArgument
                    | ErrorKind::TooManyValues
                    | ErrorKind::TooFewValues
                    | ErrorKind::ValueValidation => {
                        // user mistake → show error + usage
                        err.print().unwrap();
                        bash_symbols::BuiltinExitCode::Usage as c_int
                    }
                    _ => {
                        // unexpected / internal error
                        eprintln!("{err}");
                        bash_symbols::BuiltinExitCode::Usage as c_int
                    }
                }
            }
        }
    }

    fn get(&mut self) -> c_int {
        // This is meant to mimic yy_readline_get.
        if self.content.is_empty() || self.position >= self.content.len() {
            log::info!("---------------------- Starting app ------------------------");

            unsafe {
                if bash_symbols::job_control != 0 {
                    bash_symbols::give_terminal_to(bash_symbols::shell_pgrp, 0);
                }
            }

            // In yy_readline_get, Bash has some SIGINT handling.
            // But we put the terminal in raw mode so we're unlikely to receive SIGINTs.
            // So I don't bother with this logic.

            // I haven't bothered replicating this line either:
            //   sh_unset_nodelay_mode (fileno (rl_instream));	/* just in case */
            // Bash sets SIGCHLD to SIG_IGN, causing the kernel to auto-reap child
            // processes, which makes output()'s internal wait() fail with ECHILD.
            // Restore SIG_DFL for the entire duration of the app (covers all
            // background threads spawned for prompt widgets and agent mode), then
            // put the original disposition back once the app exits.
            // SAFETY: signal(2) only modifies the signal disposition; no other
            // thread depends on SIGCHLD disposition at this instant.
            let prev_sigchld = unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };

            let result = app::get_command(&mut self.settings);

            unsafe { libc::signal(libc::SIGCHLD, prev_sigchld) };

            // unsafe {
            //     // This doesn't seem to be strictly necessary but yy_readline_get does it here.
            //     // I think something upstream will handle it if we don't run this here.
            //     let sig = bash_symbols::terminating_signal;
            //     if sig != 0 {
            //         log::info!(
            //             "Terminating signal {} received, exiting immediately",
            //             app::signal_to_str(sig)
            //         );
            //         bash_symbols::termsig_handler(sig);
            //     }
            // }

            self.content = match result {
                app::ExitState::WithCommand(cmd) => {
                    if self.settings.tutorial_step.is_active() && cmd.trim().is_empty() {
                        self.settings.tutorial_step.next();
                        log::info!(
                            "Tutorial step advanced to {:?}",
                            self.settings.tutorial_step
                        );
                        if !self.settings.tutorial_step.is_active() {
                            self.settings.run_tutorial = false;
                        }
                    }
                    cmd.into_bytes()
                }
                app::ExitState::EOF => {
                    log::info!("App signaled EOF");
                    return bash_symbols::EOF;
                }
                app::ExitState::WithoutCommand => vec![],
            };
            log::info!("---------------------- App finished ------------------------");
            self.content.push(b'\n');
            self.position = 0;
        }

        if let Some(byte) = self.content.get(self.position) {
            self.position += 1;
            *byte as c_int
        } else {
            log::info!("End of input stream reached, returning EOF");
            bash_symbols::EOF
        }
    }

    fn unget(&mut self, _c: c_int) -> c_int {
        if self.position > 0 {
            self.position -= 1;
            self.content[self.position] as c_int
        } else {
            _c
        }
    }
}

/* Exported builtin struct */
#[unsafe(no_mangle)]
pub static mut flyline_struct: bash_symbols::BashBuiltin = bash_symbols::BashBuiltin {
    name: c"flyline".as_ptr() as *const c_char,
    function: Some(flyline_call_command),
    flags: bash_symbols::BUILTIN_ENABLED,
    long_doc: [
        c"Refer to `flyline --help` for more help.".as_ptr() as *const c_char,
        ::std::ptr::null(),
    ]
    .as_ptr(),
    short_doc: c"advanced command line editing for bash.".as_ptr() as *const c_char,
    handle: std::ptr::null(),
};

// On pre-bash-4.4 builds, register a shared-library constructor so that flyline
// is initialised as soon as the library is loaded via `enable -f`.
// On newer versions of bash `flyline_builtin_load` is called automatically by bash during enable.
#[cfg(feature = "pre_bash_4_4")]
#[ctor]
fn flyline_builtin_load_ctor() {
    let _ = flyline_load_common();
}

#[cfg(not(feature = "pre_bash_4_4"))]
#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_load(_arg: *const c_char) -> c_int {
    flyline_load_common()
}

const FLYLINE_ENV_VAR_NAME: &str = "FLYLINE_VERSION";
const FLYLINE_ENV_VAR_VALUE: &str = env!("CARGO_PKG_VERSION");

fn flyline_load_common() -> c_int {
    log::info!("flyline_builtin_load called, initializing flyline");
    // Returning 0 means the load fails
    const SUCCESS: c_int = 1;
    const FAILURE: c_int = 0;

    let already_initialized = FLYLINE_INSTANCE_PTR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some();
    if already_initialized {
        log::info!("flyline_builtin_load: already initialized, skipping");
        return SUCCESS;
    }

    logging::init().unwrap_or_else(|e| {
        eprintln!("Flyline failed to setup logging: {}", e);
    });

    // When do we want to set up flyline's input stream?
    // shell.c:main:792:set_bash_input: sets up readline if interactive && no_line_editing

    // unsafe {
    //     log::trace!(
    //         "interactive: {}, interactive_shell: {}, no_line_editing: {}",
    //         bash_symbols::interactive,
    //         bash_symbols::interactive_shell,
    //         bash_symbols::no_line_editing
    //     );
    // }

    unsafe {
        if bash_symbols::interactive_shell == 0 || bash_symbols::no_line_editing != 0 {
            log::warn!("Not an interactive shell, flyline will not be loaded");
            log::info!(
                "To avoid loading flyline in non-interactive shells, add the following to your .bashrc before the flyline enable line:\nif [[ $- != *i* ]]; then return; fi"
            );
            logging::print_logs_stderr();
            return FAILURE;
        }
    }

    // This is how we ensure that our custom input stream is used by bash instead of readline.
    // This code is run during `run_startup_files` so we can't modify bash_input directly.
    // `bash_input` is being used to read the rc files at this point. set_bash_input() has yet to be called.
    // `stream_list` contains only a sentinel input stream at this point.
    // Normally when it is popped off the list after rc files are read, readline stdin is added since
    // `with_input_from_stdin` sees that the current bash_input is of type st_stdin.
    // So we modify the sentinel node before that happens so that in set_bash_input,
    // with_input_from_stdin will see that the current bash_input is fit for purpose and not add readline stdin.

    let setup_bash_input = |bash_input: *mut bash_symbols::BashInput| {
        // Bash expects name to be heap allocated so it can free it later
        let name = c"flyline";
        let name_ptr = unsafe { bash_symbols::xmalloc_cstr(&name) };
        unsafe {
            (*bash_input).stream_type = bash_symbols::StreamType::Stdin;
            (*bash_input).name = name_ptr;
            (*bash_input).getter = Some(flyline_get_char);
            (*bash_input).ungetter = Some(flyline_unget_char);
        }

        // Store the Arc globally so C callbacks can access it
        *FLYLINE_INSTANCE_PTR
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Box::new(Flyline::new()));

        bash_funcs::set_env_var(FLYLINE_ENV_VAR_NAME, FLYLINE_ENV_VAR_VALUE).unwrap_or_else(|e| {
            log::error!(
                "Failed to set environment variable '{}': {}",
                FLYLINE_ENV_VAR_NAME,
                e
            );
        });
    };

    unsafe {
        if !bash_symbols::bash_input.name.is_null() {
            let current_input_name =
                std::ffi::CStr::from_ptr(bash_symbols::bash_input.name).to_string_lossy();

            if current_input_name.starts_with("readline") {
                log::trace!("current bash input is readline, replacing it with flyline input");
                bash_symbols::push_stream(0);
                setup_bash_input(&raw mut bash_symbols::bash_input);
                log::set_max_level(log::LevelFilter::Info);
                return SUCCESS;
            } else if current_input_name.starts_with("flyline") {
                log::trace!("current bash input is already flyline, not modifying it");
                log::set_max_level(log::LevelFilter::Info);
                return SUCCESS;
            } else {
                log::trace!("current bash input is {}", current_input_name);
            }
        }

        if !bash_symbols::stream_list.is_null() {
            // iterate through the list
            // if we find a stream of type StStdin or StNone that is already flyline, return early
            // if we find a stream of type StStdin or StNone that is not flyline, replace it with flyline
            let mut current = bash_symbols::stream_list;
            let mut idx = 0;
            while !current.is_null() {
                let stream = &*current;
                let name = if stream.bash_input.name.is_null() {
                    "?".to_string()
                } else {
                    std::ffi::CStr::from_ptr(stream.bash_input.name)
                        .to_string_lossy()
                        .into_owned()
                };
                log::trace!(
                    "stream_list[{}]: name: {}, type: {:?}",
                    idx,
                    name,
                    stream.bash_input.stream_type
                );
                if stream.bash_input.stream_type == bash_symbols::StreamType::Stdin
                    || stream.bash_input.stream_type == bash_symbols::StreamType::None
                {
                    if name.starts_with("flyline") {
                        log::trace!(
                            "Found existing flyline input stream in stream_list, not modifying stream_list"
                        );
                        log::set_max_level(log::LevelFilter::Info);
                        return SUCCESS;
                    }
                    // Replace it with flyline
                    log::trace!(
                        "Found stream_list entry with type {:?}, setting flyline input stream on this node",
                        stream.bash_input.stream_type
                    );
                    setup_bash_input(&raw mut (*current).bash_input);
                    log::set_max_level(log::LevelFilter::Info);
                    return SUCCESS;
                }

                current = stream.next;
                idx += 1;
            }
            log::error!("Could not setup flyline");
            logging::print_logs_stderr();
            return FAILURE;
        }
    }

    log::set_max_level(log::LevelFilter::Info);
    SUCCESS
}

// Its easier to just not unload on older bash versions
// Maybe I could use a fini_array function to unload, but I doubt its worth the effort.
#[cfg(not(feature = "pre_bash_4_4"))]
#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_unload() {
    log::info!("flyline_builtin_unload called, unloading flyline");

    bash_funcs::unset_env_var(FLYLINE_ENV_VAR_NAME).unwrap_or_else(|e| {
        log::error!(
            "Failed to unset environment variable '{}': {}",
            FLYLINE_ENV_VAR_NAME,
            e
        );
    });

    let had_instance = FLYLINE_INSTANCE_PTR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some();

    if !had_instance {
        return;
    }

    unsafe {
        if bash_symbols::stream_list.is_null() {
            log::trace!("stream_list is null, trying to setup readline");

            // we don't have access to yy_readline_(un)get so we can't set it directly
            // but we can call with_input_from_stdin which will set it up properly
            bash_symbols::bash_input.stream_type = bash_symbols::StreamType::None;
            bash_symbols::with_input_from_stdin();
        } else {
            let head: &mut bash_symbols::StreamSaver = &mut *bash_symbols::stream_list;
            let current_input_name =
                std::ffi::CStr::from_ptr(head.bash_input.name).to_string_lossy();
            log::trace!(
                "Found stream_list entry with name: {} and type: {:?}",
                current_input_name,
                head.bash_input.stream_type
            );
            bash_symbols::pop_stream();
        }
    }
}

// TODO try and get this working
// #[unsafe(no_mangle)]
// pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
//     println!(
//         "flyline main called. this should be called only when flyline.so is run as a standalone program."
//     );
//     0
// }
