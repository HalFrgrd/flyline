macro_rules! return_usage_error {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        return crate::bash_symbols::BuiltinExitCode::Usage as ::libc::c_int;
    }};
}

use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use clap_complete::{ArgValueCompleter, CompletionCandidate};
use libc::c_int;
use strum::VariantArray;

use crate::{
    Flyline,
    app::actions::{self},
    bash_funcs, bash_symbols, content_utils,
    cursor::{self, CursorStyleConfig},
    dparser, logging, palette, settings, tutorial,
};

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
    #[arg(long = "set-frame-rate", value_name = "FPS", value_parser = clap::value_parser!(u8).range(1..=120))]
    frame_rate: Option<u8>,
    /// Mouse capture mode (disabled, simple, smart). Default is smart.
    #[arg(long = "set-mouse-mode", value_name = "MODE", hide = true)]
    mouse_mode: Option<settings::MouseMode>,
    /// Send shell integration escape codes (OSC 133 / OSC 633): none, only-prompt-pos, or full
    #[arg(long = "send-shell-integration-codes", default_missing_value = "only-prompt-pos", num_args = 0..=1)]
    send_shell_integration_codes: Option<settings::ShellIntegrationLevel>,
    /// Whether to request the use of extended (kitty-protocol) keyboard codes during startup.
    /// Enabled by default; pass `--enable-extended-key-codes false` to
    /// disable it on terminals that misbehave when the request is sent.
    #[arg(long = "enable-extended-key-codes", default_missing_value = "true", num_args = 0..=1)]
    enable_extended_key_codes: Option<bool>,
    #[command(subcommand)]
    command: Option<Commands>,
}

pub fn complete_flyline_args(
    raw_command: &str,
    wuc: &str,
    cursor_byte: usize,
) -> anyhow::Result<Vec<clap_complete::CompletionCandidate>> {
    let command_pre_wuc = raw_command_before_word_under_cursor(raw_command, wuc, cursor_byte)?;
    log::info!("Completing flyline args after: {:?}", command_pre_wuc);

    let current_dir = std::env::current_dir().ok();
    let current_dir_asdf = current_dir.as_ref().map(|p| p.to_path_buf());

    let mut parser = dparser::DParser::from(command_pre_wuc);
    parser.walk_to_end();
    let tokens = parser
        .into_tokens()
        .into_iter()
        .map(|annoted_token| annoted_token.token)
        .collect::<Vec<_>>();

    use flash::lexer;

    let relevant_tokens = tokens
        .into_iter()
        .filter(|x| !x.kind.is_whitespace() || x.byte_range().contains(&cursor_byte))
        .filter(|x| {
            !matches!(
                x.kind,
                lexer::TokenKind::Comment
                    | lexer::TokenKind::Newline
                    | lexer::TokenKind::Quote
                    | lexer::TokenKind::SingleQuote
            )
        })
        .collect::<Vec<_>>();

    let mut args_os_string: Vec<std::ffi::OsString> = relevant_tokens
        .iter()
        .map(|t| {
            if t.kind.is_whitespace() {
                std::ffi::OsString::new()
            } else {
                std::ffi::OsString::from(&t.value)
            }
        })
        .collect();

    let dequoted_wuc = bash_funcs::dequoting_function_rust(wuc);
    let mut opt_prefix_to_strip = None;

    let merged = if let Some(last_arg) = args_os_string.last_mut() {
        let last_str = last_arg.to_string_lossy();
        if last_str.ends_with('=') || last_str.ends_with(':') {
            opt_prefix_to_strip = Some(last_str.into_owned());
            last_arg.push(&dequoted_wuc);
            true
        } else {
            false
        }
    } else {
        false
    };

    if !merged {
        args_os_string.push(std::ffi::OsString::from(dequoted_wuc));
    }

    let index = args_os_string.len() - 1;

    log::info!(
        "Completing command: cursor_byte: {}, parsed args: {:?}, index: {}, ",
        cursor_byte,
        args_os_string,
        index
    );

    let mut clap_command = FlylineArgs::command();

    match clap_complete::engine::complete(
        &mut clap_command,
        args_os_string,
        index,
        current_dir_asdf.as_deref(),
    ) {
        Ok(candidates) => {
            let processed_candidates = if let Some(prefix) = opt_prefix_to_strip {
                candidates
                    .into_iter()
                    .map(|c| {
                        let val = c.get_value().to_string_lossy();
                        if val.contains("PREFIX_DELIM") {
                            c
                        } else if let Some(suffix) = val.strip_prefix(&prefix) {
                            let new_val = format!("{}PREFIX_DELIM{}", prefix, suffix);
                            let mut new_c = clap_complete::CompletionCandidate::new(new_val);
                            if let Some(help) = c.get_help() {
                                new_c = new_c.help(Some(help.clone()));
                            }
                            new_c
                        } else {
                            c
                        }
                    })
                    .collect()
            } else {
                candidates
            };
            log::info!(
                "{:#?}",
                processed_candidates.iter().take(10).collect::<Vec<_>>()
            );
            Ok(processed_candidates)
        }
        Err(e) => {
            log::error!("Error generating bash completion: {e}");
            Err(anyhow::anyhow!("Error generating bash completion: {e}"))
        }
    }
}

fn raw_command_before_word_under_cursor<'a>(
    raw_command: &'a str,
    wuc: &str,
    cursor_byte: usize,
) -> anyhow::Result<&'a str> {
    if cursor_byte > raw_command.len() {
        anyhow::bail!("cursor is outside raw command");
    }

    if wuc.is_empty() {
        if !raw_command.is_char_boundary(cursor_byte) {
            anyhow::bail!("cursor is not on a char boundary");
        }
        return Ok(&raw_command[..cursor_byte]);
    }

    raw_command
        .match_indices(wuc)
        .find_map(|(start, _)| {
            let end = start + wuc.len();
            (start <= cursor_byte && cursor_byte <= end).then_some(&raw_command[..start])
        })
        .ok_or_else(|| anyhow::anyhow!("word under cursor not found at cursor"))
}

fn possible_interpolate_easing_completions(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    cursor::CursorEasing::VARIANTS
        .into_iter()
        .filter(|s| s.as_ref().starts_with(current.to_string_lossy().as_ref()))
        .map(|s| {
            let description = content_utils::easing_animation_frames(*s);
            let description_str = description
                .into_iter()
                .map(|line| {
                    line.iter()
                        .map(content_utils::span_to_ansi)
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\t");

            CompletionCandidate::new(s.as_ref().to_string()).help(Some(description_str.into()))
        })
        .collect()
}

fn possible_effect_easing_completions(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    cursor::CursorEasing::VARIANTS
        .into_iter()
        .filter(|s| s.as_ref().starts_with(current.to_string_lossy().as_ref()))
        .map(|s| {
            let description = cursor::cursor_effect_animation_frames(
                *s,
                cursor::CursorConfig::default().effect_speed,
            );
            let description_str = description
                .into_iter()
                .map(|line| {
                    line.iter()
                        .map(content_utils::span_to_ansi)
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\t");

            CompletionCandidate::new(s.as_ref().to_string()).help(Some(description_str.into()))
        })
        .collect()
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show version information.
    #[command(name = "version")]
    Version {
        /// Copy version information to clipboard
        #[arg(long)]
        copy: bool,
    },
    /// Dump all in-memory log entries to stdout.
    #[command(name = "dump", verbatim_doc_comment)]
    Dump {
        /// Only show log entries from the last duration (e.g. 5s, 2m, 1h)
        #[arg(long)]
        last: Option<String>,
    },
    /// Print a timestamp.
    ///
    /// With no flags, prints nanoseconds since the Unix epoch.
    /// With --format, formats the current local time using a Chrono strftime
    /// format string (e.g. "%Y-%m-%dT%H:%M:%S").
    ///
    /// To display elapsed time since the last command, use the
    /// `last-command-duration` prompt widget instead:
    ///   flyline create-prompt-widget last-command-duration
    ///
    /// Examples:
    ///   flyline time
    ///   flyline time --format "%Y-%m-%dT%H:%M:%S"
    #[command(name = "time", verbatim_doc_comment)]
    Time {
        /// Format string passed to Chrono's `strftime` formatter.
        /// When omitted, prints nanoseconds since the Unix epoch.
        #[arg(long = "format", value_name = "FORMAT")]
        format: Option<String>,
    },
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
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL, and their _FINAL counterparts) are replaced
    /// with the widget output on every render.
    ///
    /// Widget types:
    ///   animation   Cycles through a list of frames at a given fps.
    ///   mouse-mode             Shows different text depending on whether mouse capture is enabled.
    ///   copy-buffer            Shows clickable text that copies the current buffer to the clipboard.
    ///   custom                 Runs a shell command and displays its output.
    ///   last-command-duration  Shows how long ago the flyline app last closed.
    ///
    /// Examples:
    ///   flyline create-prompt-widget animation --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
    ///   flyline create-prompt-widget animation --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'
    ///   flyline create-prompt-widget mouse-mode --name FLYLINE_MOUSE_MODE 'mouse is enabled' 'mouse is disabled'
    ///   flyline create-prompt-widget copy-buffer --name COPY_BUFFER '[copy]'
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_something.sh' --placeholder prev
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_something.sh' --block
    ///   flyline create-prompt-widget last-command-duration
    ///   flyline create-prompt-widget last-command-duration --name LAST_CMD_DUR
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
    ///   markdown-code, key-sequence-style, selected-text, bash-reserved
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
        #[arg(value_name = "NAME=STYLE", add = ArgValueCompleter::new(palette::possible_style_name_completions))]
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
        #[arg(long, value_name = "EASING", add = ArgValueCompleter::new(possible_interpolate_easing_completions))]
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
        #[arg(long, value_name = "EASING", add = ArgValueCompleter::new(possible_effect_easing_completions))]
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
    /// Configure suggestion behavior.
    ///
    /// Examples:
    ///   flyline suggestions --auto-suggest false
    ///   flyline suggestions --num-suggestion-rows 10
    ///   flyline suggestions --auto-suggest true --num-suggestion-rows 12
    #[command(name = "suggestions", verbatim_doc_comment)]
    Suggestions {
        /// Optional subcommand for suggestion actions.
        #[command(subcommand)]
        subcommand: Option<SuggestionsSubcommands>,

        /// Enable or disable auto-suggest (auto-started tab completion suggestions).
        #[arg(long = "auto-suggest", default_missing_value = "true", num_args = 0..=1)]
        auto_suggest: Option<bool>,
        /// Enable or disable flycomp for synthesizing shell completions when no useful compspec is found.
        #[arg(long = "use-flycomp", default_missing_value = "true", num_args = 0..=1)]
        use_flycomp: Option<bool>,
        /// How to sort suggestions when fuzzy scores are tied (mtime, alphabetical).
        #[arg(long = "sort-order", value_name = "ORDER")]
        sort_order: Option<settings::SuggestionSortOrder>,
        /// Maximum number of suggestion rows to render for tab-completion lists.
        #[arg(long = "num-suggestion-rows", value_name = "NUM")]
        num_suggestion_rows: Option<u16>,
        /// Directory where flycomp output should be saved.
        /// You should source the completions from this directory in your bashrc so flyline can use them next time.
        #[arg(long = "flycomp-output", value_name = "DIR")]
        flycomp_output: Option<String>,
        /// Blacklist of command words for which flycomp prompt should be bypassed.
        #[arg(long = "flycomp-blacklist", value_name = "COMMANDS", num_args = 1..)]
        flycomp_blacklist: Option<Vec<String>>,
    },
    /// Configure mouse options and debugging.
    #[command(name = "mouse", verbatim_doc_comment)]
    Mouse {
        /// Show the last mouse event above the prompt.
        #[arg(long = "debug", default_missing_value = "true", num_args = 0..=1)]
        debug: Option<bool>,
        /// Whether to change the mouse cursor shape depending on what is hovered.
        #[arg(long = "change-shape", default_missing_value = "true", num_args = 0..=1)]
        change_shape: Option<bool>,
        /// Mouse capture mode (disabled, simple, smart).
        #[arg(long = "mode", value_name = "MODE")]
        mode: Option<settings::MouseMode>,
    },
    /// Performance profiling commands: start, stop, or dump stats.
    #[command(name = "perf", verbatim_doc_comment)]
    Perf {
        #[command(subcommand)]
        subcommand: PerfSubcommands,
    },
    /// Display the changelog of user-facing changes.
    ///
    /// Examples:
    ///   flyline changelog
    #[command(name = "changelog", verbatim_doc_comment)]
    Changelog,
    /// Display instructions to upgrade flyline.
    ///
    /// Examples:
    ///   flyline upgrade
    #[command(name = "upgrade", verbatim_doc_comment)]
    Upgrade,
}

#[derive(Subcommand, Debug)]
enum SuggestionsSubcommands {
    /// Set fuzzy matching mode (all, none, no folders).
    #[command(name = "set-fuzzy-mode", verbatim_doc_comment)]
    SetFuzzyMode {
        /// The fuzzy mode to set (all, none, no folders).
        #[arg(value_name = "MODE")]
        mode: settings::FuzzyMode,
    },
}

#[derive(Subcommand, Debug)]
enum PerfSubcommands {
    /// Start recording performance metrics.
    #[command(name = "start")]
    Start,
    /// Stop recording performance metrics.
    #[command(name = "stop")]
    Stop,
    /// Dump aggregated performance metrics to stdout.
    #[command(name = "dump")]
    Dump,
}

#[derive(Subcommand, Debug)]
enum KeySubcommands {
    /// Bind a key sequence to an action, optionally guarded by a context expression.
    ///
    /// KEY_SEQUENCE is a key combination such as "Ctrl+Enter" or "Alt+Left".
    /// CONTEXT_AND_ACTION has the form `<contextExpr>=<actionName>`, where
    /// `<contextExpr>` is a `+`-separated chain of camelCase context variables
    /// (each optionally prefixed with `!` to negate).  Use `always` for
    /// unconditional bindings.  Parentheses and `||` are not supported.
    ///
    /// Available context variables: always, bufferIsEmpty, fuzzyHistorySearch,
    ///   tabCompletionWaiting, tabCompletion, tabCompletionAvailable,
    ///   tabCompletionOneResult, tabCompletionMultiColAvailable,
    ///   tabCompletionNoFilteredResults, tabCompletionNoResults,
    ///   agentModeWaiting, agentOutputSelection, agentModeError,
    ///   inlineSuggestionAvailable, cursorAtEnd, cursorAtEndTrimmed,
    ///   cursorAtStart, promptDirSelection, textSelected, multilineBuffer,
    ///   bufferHasAgentModePrefix, editingBufferMode.
    ///
    /// Examples:
    ///   flyline key bind Ctrl+Enter always=submitOrNewline
    ///   flyline key bind Tab inlineSuggestionAvailable+cursorAtEnd=inlineSuggestionAccept
    ///   flyline key bind Alt+Left always=moveLeftOneWordPart
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
        #[arg(add = ArgValueCompleter::new(actions::key_sequence_completer))]
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
        #[arg(add = ArgValueCompleter::new(actions::remap_key_completer))]
        from: String,
        /// The key or modifier to remap to (e.g. "z", "ctrl").
        #[arg(add = ArgValueCompleter::new(actions::remap_key_completer))]
        to: String,
    },
}

#[derive(Subcommand, Debug)]
enum LogSubcommands {
    /// Dump all in-memory log entries to stdout.
    ///
    /// Examples:
    ///   flyline log dump
    ///   flyline log dump --last 5s
    #[command(name = "dump", verbatim_doc_comment)]
    Dump {
        /// Only show log entries from the last duration (e.g. 5s, 2m, 1h)
        #[arg(long)]
        last: Option<String>,
    },
    /// Copy in-memory log entries to the clipboard.
    ///
    /// Examples:
    ///   flyline log copy
    ///   flyline log copy --last 5s
    #[command(name = "copy", verbatim_doc_comment)]
    Copy {
        /// Only copy log entries from the last duration (e.g. 5s, 2m, 1h)
        #[arg(long)]
        last: Option<String>,
    },
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
        #[arg(value_name = "FILEPATH|stderr|terminal")]
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
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL, and their _FINAL counterparts) are replaced
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
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL, and their _FINAL counterparts) are replaced
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
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL, and their _FINAL counterparts) are replaced
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
    ///   # Non-blocking with previous output placeholder while the new output is being computed.
    ///   flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_slow_git_metrics.sh' --placeholder prev
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
    /// Show how long ago the flyline app last closed in the prompt.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL, and their _FINAL counterparts) are replaced
    /// with the elapsed duration on every render.  The format is compact and
    /// human-readable, for example: 9.2s, 1m23s, 1h02m03s, 1d20h43m.
    ///
    /// The widget text inherits (and may override) the style of the prompt span
    /// it is embedded in, just like other widgets.
    ///
    /// Examples:
    ///   flyline create-prompt-widget last-command-duration
    ///   # Now use FLYLINE_LAST_COMMAND_DURATION in your prompt:
    ///   RPS1=' FLYLINE_LAST_COMMAND_DURATION'
    ///
    ///   flyline create-prompt-widget last-command-duration --name MY_DURATION
    #[command(name = "last-command-duration", verbatim_doc_comment)]
    LastCommandDuration {
        /// Name to embed in prompt strings as the widget placeholder.
        /// Defaults to `FLYLINE_LAST_COMMAND_DURATION`.
        #[arg(long, default_value = "FLYLINE_LAST_COMMAND_DURATION")]
        name: String,
    },
}
impl Flyline {
    pub(crate) fn call(&mut self, words: *const bash_symbols::WordList) -> c_int {
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
                    show_version(false);
                    return bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int;
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
                    Some(Commands::Version { copy }) => {
                        show_version(copy);
                        return bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int;
                    }
                    Some(Commands::Dump { last }) => {
                        use std::io::Write;
                        match crate::logging::get_filtered_logs(last.as_deref()) {
                            Ok(entries) => {
                                let stdout = std::io::stdout();
                                let mut out = stdout.lock();
                                for entry in entries {
                                    if let Err(e) = writeln!(out, "{}", entry) {
                                        eprintln!("Failed to write log entry: {}", e);
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to retrieve logs: {}", e);
                                return bash_symbols::BuiltinExitCode::Usage as c_int;
                            }
                        }
                        return bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int;
                    }
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
                            return_usage_error!(
                                "flyline set-agent-mode: --command must not be empty"
                            );
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
                                return_usage_error!(
                                    "flyline create-prompt-widget animation: --fps must be greater than 0 (got {}); animation '{}' not registered",
                                    fps,
                                    name
                                );
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
                                settings::PromptWidget::MouseMode {
                                    name,
                                    enabled_text,
                                    disabled_text,
                                },
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
                                settings::PromptWidget::CopyBuffer { name, text },
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
                                return_usage_error!(
                                    "flyline create-prompt-widget custom: --command must not be empty"
                                );
                            }
                            if let Some(ms) = block
                                && ms < 0
                            {
                                return_usage_error!(
                                    "flyline create-prompt-widget custom: --block timeout must be non-negative (got {})",
                                    ms
                                );
                            }
                            let placeholder_spec = match placeholder {
                                None => None,
                                Some(ref s) if s == "prev" => Some(settings::Placeholder::Prev),
                                Some(ref s) => match s.parse::<usize>() {
                                    Ok(n) => Some(settings::Placeholder::Spaces(n)),
                                    Err(_) => {
                                        return_usage_error!(
                                            "flyline create-prompt-widget custom: --placeholder must be a number or 'prev', got {:?}",
                                            s
                                        );
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
                        PromptWidgetSubcommands::LastCommandDuration { name } => {
                            log::info!("Registering last-command-duration widget '{}'", name);
                            self.settings.custom_prompt_widgets.insert(
                                name.clone(),
                                settings::PromptWidget::LastCommandDuration { name },
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
                            let Some((name, style_str)) = spec.split_once('=') else {
                                return_usage_error!(
                                    "flyline set-style: argument must be NAME=STYLE, got {:?}",
                                    spec
                                );
                            };
                            let kind = match name.parse::<palette::PaletteStyleKind>() {
                                Ok(k) => k,
                                Err(_) => {
                                    return_usage_error!(
                                        "flyline set-style: unknown style name {:?}. Run 'flyline set-style --help' for valid names.",
                                        name
                                    );
                                }
                            };
                            match palette::parse_str_to_style(style_str) {
                                Ok(style) => {
                                    self.settings.colour_palette.set(kind, style);
                                    log::info!("{} style set to {:?}", name, style_str);
                                }
                                Err(e) => {
                                    return_usage_error!(
                                        "flyline set-style: invalid style for {:?}: {}",
                                        name,
                                        e
                                    );
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
                                        return_usage_error!("flyline key bind: {}", e);
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
                                        return_usage_error!(
                                            "flyline key remap: failed to parse remap '{}' -> '{}': {}",
                                            from,
                                            to,
                                            e
                                        );
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                    Some(Commands::Mouse {
                        debug,
                        change_shape,
                        mode,
                    }) => {
                        if let Some(enabled) = debug {
                            log::info!("Mouse debug mode enabled: {}", enabled);
                            self.settings.mouse_debug = enabled;
                        }
                        if let Some(enabled) = change_shape {
                            log::info!("Mouse change shape enabled: {}", enabled);
                            self.settings.mouse_change_shape = enabled;
                        }
                        if let Some(m) = mode {
                            log::info!("Mouse mode set to {:?}", m);
                            self.settings.mouse_mode = m;
                        }
                    }
                    None => {}
                    Some(Commands::Log { subcommand }) => {
                        match subcommand {
                            LogSubcommands::Dump { last } => {
                                match crate::logging::get_filtered_logs(last.as_deref()) {
                                    Ok(entries) => {
                                        use std::io::Write;
                                        let stdout = std::io::stdout();
                                        let mut out = stdout.lock();
                                        for entry in entries {
                                            if let Err(e) = writeln!(out, "{}", entry) {
                                                eprintln!("Failed to write log entry: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to retrieve logs: {}", e);
                                    }
                                }
                            }
                            LogSubcommands::Copy { last } => {
                                match crate::logging::get_filtered_logs(last.as_deref()) {
                                    Ok(entries) => {
                                        let len = entries.len();
                                        let logs_to_copy = if len > 10_000 {
                                            entries[len - 10_000..].to_vec()
                                        } else {
                                            entries
                                        };
                                        let joined_logs = logs_to_copy.join("\n");
                                        if let Err(e) = crossterm::execute!(
                                        std::io::stdout(),
                                        crossterm::clipboard::CopyToClipboard::to_clipboard_from(joined_logs)
                                    ) {
                                        eprintln!("Failed to copy logs to clipboard via OSC 52: {}", e);
                                    } else {
                                        println!("Copied {} log lines!", logs_to_copy.len());
                                    }
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to retrieve logs: {}", e);
                                    }
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
                        }
                    }
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
                    Some(Commands::Suggestions {
                        subcommand,
                        auto_suggest,
                        use_flycomp,
                        sort_order,
                        num_suggestion_rows,
                        flycomp_output,
                        flycomp_blacklist,
                    }) => {
                        if let Some(sub) = subcommand {
                            match sub {
                                SuggestionsSubcommands::SetFuzzyMode { mode } => {
                                    log::info!("Fuzzy mode set to {:?}", mode);
                                    self.settings.fuzzy_mode = mode;
                                }
                            }
                        }
                        if let Some(list) = flycomp_blacklist {
                            log::info!("Flycomp blacklist set to {:?}", list);
                            self.settings.flycomp_blacklist = list.into_iter().collect();
                        }
                        if let Some(enabled) = auto_suggest {
                            log::info!("Auto tab-completion suggestions set to {}", enabled);
                            self.settings.auto_suggest = enabled;
                        }
                        if let Some(enabled) = use_flycomp {
                            log::info!("Use flycomp set to {}", enabled);
                            self.settings.use_flycomp = enabled;
                        }
                        if let Some(order) = sort_order {
                            log::info!("Suggestion sort order set to {:?}", order);
                            self.settings.suggestion_sort_order = order;
                        }
                        if let Some(num) = num_suggestion_rows {
                            if num == 0 {
                                return_usage_error!(
                                    "flyline suggestions: --num-suggestion-rows must be greater than 0"
                                );
                            }
                            log::info!("Suggestion row limit set to {}", num);
                            self.settings.num_suggestion_rows = num;
                        }
                        if let Some(path) = flycomp_output {
                            log::info!("Flycomp output directory set to '{}'", path);
                            self.settings.flycomp_output = Some(path);
                        }
                    }
                    Some(Commands::Time { format }) => {
                        if let Some(fmt) = format {
                            let has_error = chrono::format::strftime::StrftimeItems::new(&fmt)
                                .any(|item| matches!(item, chrono::format::Item::Error));
                            if has_error {
                                return_usage_error!(
                                    "flyline time: invalid Chrono format string: {:?}",
                                    fmt
                                );
                            }
                            println!("{}", chrono::Local::now().format(&fmt));
                        } else {
                            let ns = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            println!("{}", ns);
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
                        // If the user configures flyline-only options without explicitly setting the backend,
                        // and we defaulted to terminal (e.g. on kitty), automatically switch to flyline.
                        if backend.is_none()
                            && self.settings.cursor_config.is_backend_unset()
                            && (style.is_some()
                                || effect.is_some()
                                || effect_speed.is_some()
                                || effect_easing.is_some())
                        {
                            log::info!(
                                "Auto-switching cursor backend to Flyline for configured options"
                            );
                            self.settings
                                .cursor_config
                                .set_backend(Some(cursor::CursorBackend::Flyline));
                        }

                        // set backend first since it affects the validity of other options
                        if let Some(b) = backend {
                            log::info!("Cursor backend set to {:?}", b);
                            self.settings.cursor_config.set_backend(Some(b));
                            if b == cursor::CursorBackend::Terminal
                                && (style.is_some()
                                    || effect.is_some()
                                    || effect_speed.is_some()
                                    || effect_easing.is_some())
                            {
                                return_usage_error!(
                                    "flyline set-cursor: --style, --effect, --effect-speed, and --effect-easing require --backend flyline"
                                );
                            }
                        }

                        // Helper closure: every flyline-only option emits the same error.
                        // Returning a `bool` lets callers chain it with the option-presence check.
                        let backend_is_terminal = self.settings.cursor_config.backend()
                            == cursor::CursorBackend::Terminal;

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
                                        return_usage_error!(
                                            "flyline set-cursor: --interpolate must be a positive number or 'none' (got {:?})",
                                            interp_str
                                        );
                                    }
                                }
                            }
                        }

                        if let Some(easing) = interpolate_easing {
                            log::info!("Cursor interpolation easing set to {:?}", easing);
                            self.settings.cursor_config.interpolate_easing = easing;
                        }

                        if let Some(style_str) = style {
                            if backend_is_terminal {
                                return_usage_error!(
                                    "flyline set-cursor: --style requires --backend flyline"
                                );
                            }
                            match palette::parse_cursor_style_str(&style_str) {
                                Ok(s) => {
                                    log::info!("Cursor style set to {:?}", s);
                                    self.settings.cursor_config.style = s;
                                }
                                Err(e) => {
                                    return_usage_error!(
                                        "flyline set-cursor: invalid --style {:?}: {}",
                                        style_str,
                                        e
                                    );
                                }
                            }
                        }

                        if let Some(eff) = effect {
                            if backend_is_terminal {
                                return_usage_error!(
                                    "flyline set-cursor: --effect requires --backend flyline"
                                );
                            }
                            if eff == cursor::CursorEffect::Fade
                                && let CursorStyleConfig::Custom(style) =
                                    self.settings.cursor_config.style
                                && !matches!(style.bg, Some(ratatui::style::Color::Rgb(..)))
                            {
                                return_usage_error!(
                                    "flyline set-cursor: --effect fade requires a custom style with an RGB background color (e.g. '#ff0000')"
                                );
                            }
                            log::info!("Cursor effect set to {:?}", eff);
                            self.settings.cursor_config.effect = eff;
                        }

                        if let Some(speed) = effect_speed {
                            if backend_is_terminal {
                                return_usage_error!(
                                    "flyline set-cursor: --effect-speed requires --backend flyline"
                                );
                            }
                            if speed > 0.0 {
                                log::info!("Cursor effect speed set to {}", speed);
                                self.settings.cursor_config.effect_speed = speed;
                            } else {
                                return_usage_error!(
                                    "flyline set-cursor: --effect-speed must be positive (got {})",
                                    speed
                                );
                            }
                        }

                        if let Some(easing) = effect_easing {
                            if backend_is_terminal {
                                return_usage_error!(
                                    "flyline set-cursor: --effect-easing requires --backend flyline"
                                );
                            }
                            log::info!("Cursor effect easing set to {:?}", easing);
                            self.settings.cursor_config.effect_easing = easing;
                        }
                    }
                    Some(Commands::Perf { subcommand }) => match subcommand {
                        PerfSubcommands::Start => {
                            crate::perf::start_recording();
                            println!("Performance recording started.");
                        }
                        PerfSubcommands::Stop => {
                            crate::perf::stop_recording();
                            println!("Performance recording stopped.");
                        }
                        PerfSubcommands::Dump => {
                            crate::perf::dump_to_stdout();
                        }
                    },
                    Some(Commands::Changelog) => {
                        let content = crate::changelog::CHANGELOG;
                        let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
                        let mut parts = pager.split_whitespace();
                        if let Some(bin) = parts.next() {
                            let args: Vec<&str> = parts.collect();
                            let mut cmd = std::process::Command::new(bin);
                            cmd.args(&args);
                            if bin == "less" && args.is_empty() {
                                cmd.args(["-R", "-F", "-X"]);
                            }
                            cmd.stdin(std::process::Stdio::piped());
                            match cmd.spawn() {
                                Ok(mut child_proc) => {
                                    if let Some(mut stdin) = child_proc.stdin.take() {
                                        use std::io::Write;
                                        if stdin.write_all(content.as_bytes()).is_ok() {
                                            drop(stdin); // close stdin to signal EOF to the pager
                                            let _ = child_proc.wait();
                                        }
                                    }
                                }
                                Err(_) => {
                                    println!("{}", content);
                                }
                            }
                        } else {
                            println!("{}", content);
                        }
                    }
                    Some(Commands::Upgrade) => {
                        println!("Flyline is a purely offline piece of software. Please run:");
                        println!(
                            "curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh"
                        );
                        self.settings.initial_buffer = Some("curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh".to_string());
                    }
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
}

fn get_os_pretty_name() -> Option<String> {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                let name = rest.trim_matches('"').trim_matches('\'');
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn get_libc_version() -> Option<String> {
    let output = std::process::Command::new("ldd")
        .arg("--version")
        .output()
        .ok()?;

    // Check stdout first
    let out_str = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = out_str.lines().next() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // Fallback to stderr (e.g. musl ldd outputting to stderr)
    let err_str = String::from_utf8_lossy(&output.stderr);
    if let Some(line) = err_str.lines().next() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn get_os_info() -> String {
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) == 0 {
            let sysname = std::ffi::CStr::from_ptr(uts.sysname.as_ptr()).to_string_lossy();
            let release = std::ffi::CStr::from_ptr(uts.release.as_ptr()).to_string_lossy();
            let machine = std::ffi::CStr::from_ptr(uts.machine.as_ptr()).to_string_lossy();
            let os_name = if sysname == "Darwin" {
                "macOS".to_string()
            } else {
                get_os_pretty_name().unwrap_or_else(|| sysname.into_owned())
            };
            format!("{} (kernel {} {})", os_name, release, machine)
        } else {
            "Unknown OS".to_string()
        }
    }
}

fn get_system_linker_version() -> Option<String> {
    let output = std::process::Command::new("ld").arg("-v").output().ok()?;

    // Check stdout first
    let out_str = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = out_str.lines().next() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // Fallback to stderr (Apple's ld -v prints to stderr)
    let err_str = String::from_utf8_lossy(&output.stderr);
    if let Some(line) = err_str.lines().next() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn get_dynamic_linker_version() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        get_libc_version()
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sw_vers").output().ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut product = "macOS".to_string();
            let mut build = "".to_string();
            for line in stdout.lines() {
                if let Some(version) = line.strip_prefix("ProductVersion:") {
                    product = format!("macOS {}", version.trim());
                } else if let Some(b) = line.strip_prefix("BuildVersion:") {
                    build = format!(" (Build {})", b.trim());
                }
            }
            return Some(format!("dyld (tied to {}{})", product, build));
        }
        None
    }
    #[cfg(target_os = "freebsd")]
    {
        let output = std::process::Command::new("freebsd-version")
            .arg("-u")
            .output()
            .ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    return Some(format!("rtld (tied to FreeBSD userland {})", trimmed));
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
    {
        None
    }
}

fn get_cpu_info() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name")
                    || line.starts_with("Processor")
                    || line.starts_with("Hardware")
                {
                    if let Some(pos) = line.find(':') {
                        let model = line[pos + 1..].trim().to_string();
                        if !model.is_empty() {
                            let cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
                            let cores_str = if cores > 0 {
                                format!(" ({} cores)", cores)
                            } else {
                                String::new()
                            };
                            return format!("{}{}", model, cores_str);
                        }
                    }
                }
            }
        }
    }
    format!("{} architecture", std::env::consts::ARCH)
}

fn get_bash_version() -> String {
    crate::bash_funcs::get_envvar_value("BASH_VERSION").unwrap_or_else(|| "unknown".to_string())
}

fn show_version(copy: bool) {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = env!("GIT_HASH");
    let build_mode = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let build_time = env!("BUILD_TIME");
    let build_target = env!("BUILD_TARGET");
    let rustc_version = env!("RUSTC_VERSION");

    let bash_version = get_bash_version();
    let shell =
        crate::bash_funcs::get_envvar_value("SHELL").unwrap_or_else(|| "unknown".to_string());
    let term = crate::bash_funcs::get_envvar_value("TERM").unwrap_or_else(|| "unknown".to_string());
    let term_program = crate::bash_funcs::get_envvar_value("TERM_PROGRAM")
        .unwrap_or_else(|| "unknown".to_string());
    let term_program_version = crate::bash_funcs::get_envvar_value("TERM_PROGRAM_VERSION")
        .unwrap_or_else(|| "unknown".to_string());
    let lang = crate::bash_funcs::get_envvar_value("LANG").unwrap_or_else(|| "unknown".to_string());
    let lc_all =
        crate::bash_funcs::get_envvar_value("LC_ALL").unwrap_or_else(|| "unknown".to_string());
    let lc_ctype =
        crate::bash_funcs::get_envvar_value("LC_CTYPE").unwrap_or_else(|| "unknown".to_string());

    let os_info = get_os_info();
    let sys_linker = get_system_linker_version().unwrap_or_else(|| "unknown".to_string());
    let dyn_linker = get_dynamic_linker_version().unwrap_or_else(|| "unknown".to_string());
    let cpu_info = get_cpu_info();

    let version_text = format!(
        "flyline {}\n\
         git hash: {}\n\
         build mode: {}\n\
         build datetime: {}\n\
         build target: {}\n\
         rustc version: {}\n\
         \n\
         Running in: Bash {}\n\
         shell: {}\n\
         \n\
         Running in: {}\n\
         program: {}\n\
         program version: {}\n\
         locale: {} (LC_ALL: {}, LC_CTYPE: {})\n\
         \n\
         Running in: {}\n\
         system linker: {}\n\
         dynamic linker: {}\n\
         \n\
         Running on: {}\n\
         \n\
         Running in: a simulation",
        version,
        git_hash,
        build_mode,
        build_time,
        build_target,
        rustc_version,
        bash_version,
        shell,
        term,
        term_program,
        term_program_version,
        lang,
        lc_all,
        lc_ctype,
        os_info,
        sys_linker,
        dyn_linker,
        cpu_info
    );

    println!("{}", version_text);

    if copy {
        if let Err(e) = crossterm::execute!(
            std::io::stdout(),
            crossterm::clipboard::CopyToClipboard::to_clipboard_from(version_text)
        ) {
            log::error!("Failed to copy version text to clipboard via OSC 52: {}", e);
        }
        println!();
        println!("\x1b[32mCopied to clipboard!\x1b[0m");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_subcommand_completions() {
        let raw_cmd = "flyline perf ";
        let wuc = "";
        let cursor_byte = raw_cmd.len();
        let comps = complete_flyline_args(raw_cmd, wuc, cursor_byte).unwrap();
        let values: Vec<String> = comps
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect();
        assert!(values.contains(&"start".to_string()));
        assert!(values.contains(&"stop".to_string()));
        assert!(values.contains(&"dump".to_string()));
    }

    #[test]
    fn test_flyline_key_bind_completion() {
        let raw_cmd = "flyline key bind BackTab agentModeError=";
        let wuc = "";
        let cursor_byte = raw_cmd.len();
        let comps = complete_flyline_args(raw_cmd, wuc, cursor_byte).unwrap();
        let values: Vec<String> = comps
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_flyline_key_list_completion() {
        let raw_cmd = "flyline key list Ctrl+";
        let wuc = "Ctrl+";
        let cursor_byte = raw_cmd.len();
        let comps = complete_flyline_args(raw_cmd, wuc, cursor_byte).unwrap();
        let values: Vec<String> = comps
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect();
        assert!(!values.is_empty());
        assert!(values.iter().any(|v| v.contains("Ctrl+")));
    }

    #[test]
    fn test_flyline_option_completion() {
        let raw_cmd = "flyline --show-animations=t";
        let wuc = "t";
        let cursor_byte = raw_cmd.len();
        let comps = complete_flyline_args(raw_cmd, wuc, cursor_byte).unwrap();
        let values: Vec<String> = comps
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect();
        assert!(
            values.contains(&"--show-animations=PREFIX_DELIMtrue".to_string())
                || values.contains(&"--show-animations=PREFIX_DELIMfalse".to_string())
        );
    }

    #[test]
    fn test_version_helpers() {
        let os_info = get_os_info();
        let cpu_info = get_cpu_info();
        let bash_ver = get_bash_version();

        assert!(!os_info.is_empty());
        assert!(!cpu_info.is_empty());
        assert!(!bash_ver.is_empty());
    }
}
