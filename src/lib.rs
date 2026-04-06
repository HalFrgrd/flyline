use clap::{CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use clap_complete::{Shell, generate};
use libc::{c_char, c_int};
use ratatui::style::Style;
use std::sync::Mutex;

use crate::app::actions::{self, possible_action_names};

mod active_suggestions;
mod agent_mode;
mod app;
mod bash_funcs;
mod bash_symbols;
mod command_acceptance;
mod command_rebuild;
mod content_builder;
mod cursor_animation;
mod dparser;
mod history;
mod iter_first_last;
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

#[derive(ValueEnum, Clone, Debug)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
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
    /// Dump in-memory logs to file. Optionally specify a PATH; if omitted, a timestamped file is
    /// created in the current directory
    #[arg(long = "dump-logs", value_name = "PATH", default_missing_value = "", num_args = 0..=1)]
    dump_logs: Option<String>,
    /// Dump current logs to PATH and append new logs. Use `stderr` to stream to standard error
    #[arg(long = "stream-logs", value_name = "PATH")]
    stream_logs: Option<String>,
    /// Set the logging level
    #[arg(long = "log-level", value_name = "LEVEL")]
    log_level: Option<LogLevel>,
    /// Load Zsh history in addition to Bash history. Optionally specify a PATH to the Zsh history
    /// file; if omitted, defaults to $HOME/.zsh_history
    #[arg(long = "load-zsh-history", value_name = "PATH", default_missing_value = "", num_args = 0..=1)]
    load_zsh_history: Option<String>,
    /// Enable or disable tutorial mode with hints for first-time users.
    /// Use `--tutorial-mode false` to disable.
    #[arg(long = "tutorial-mode", default_missing_value = "true", num_args = 0..=1)]
    tutorial_mode: Option<bool>,
    /// Show animations
    #[arg(long = "show-animations", default_missing_value = "true", num_args = 0..=1)]
    show_animations: Option<bool>,
    /// Show inline history suggestions
    #[arg(long = "show-inline-history", default_missing_value = "true", num_args = 0..=1)]
    show_inline_history: Option<bool>,
    /// Enable automatic closing character insertion (e.g. insert `)` after `(`)
    #[arg(long = "auto-close-chars", default_missing_value = "true", num_args = 0..=1)]
    auto_close_chars: Option<bool>,
    /// Use the terminal emulator's cursor instead of rendering a custom cursor
    #[arg(long = "use-term-emulator-cursor", default_missing_value = "full", num_args = 0..=1)]
    use_term_emulator_cursor: Option<settings::UseTermEmulatorCursor>,
    /// Run matrix animation in the terminal background
    #[arg(long = "matrix-animation", default_missing_value = "true", num_args = 0..=1)]
    matrix_animation: Option<bool>,
    /// Render frame rate in frames per second (1–120, default 30)
    #[arg(long = "frame-rate", value_name = "FPS", value_parser = clap::value_parser!(u8).range(1..=120))]
    frame_rate: Option<u8>,
    /// Mouse capture mode (disabled, simple, smart). Default is smart.
    #[arg(long = "mouse-mode", value_name = "MODE")]
    mouse_mode: Option<settings::MouseMode>,
    /// Send shell integration escape codes (OSC 133 / OSC 633)
    #[arg(long = "send-shell-integration-codes", default_missing_value = "true", num_args = 0..=1)]
    send_shell_integration_codes: Option<bool>,
    // Only for integration tests
    #[cfg(feature = "integration-tests")]
    #[arg(long = "run-tab-completion-tests")]
    run_tab_completion_tests: bool,
    #[command(subcommand)]
    command: Option<Commands>,
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
    ///   (N.B. `--command` should be the final flag since it consumes all remaining arguments)
    ///   flyline agent-mode \
    ///     --system-prompt "Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \
    ///     --command copilot --reasoning-effort low --prompt
    ///   flyline agent-mode --trigger-prefix ": " --command copilot --reasoning-effort low --prompt
    ///
    /// See https://github.com/HalFrgrd/flyline/blob/master/examples/agent_mode.sh for more details and example usage.
    #[command(name = "agent-mode", verbatim_doc_comment)]
    AgentMode {
        /// Optional system prompt prepended to the buffer.
        /// The subprocess receives "<system-prompt>\n<buffer>" as its final argument.
        #[arg(long = "system-prompt")]
        system_prompt: Option<String>,
        /// Optional trigger prefix. When set, pressing Enter with a buffer that
        /// starts with this prefix activates agent mode (the prefix is stripped).
        #[arg(long = "trigger-prefix")]
        trigger_prefix: Option<String>,
        /// Command (and arguments) to invoke. The current buffer is appended as the
        /// final argument when Alt+Enter is pressed.
        #[arg(long = "command", num_args = 1.., allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
    /// Create a custom prompt animation.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with the current animation frame on every render.  Frames may include
    /// ANSI colour sequences written as `\e` (e.g. `\e[33m`).
    ///
    /// Examples:
    ///   flyline create-anim --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
    ///   flyline create-anim --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'
    ///
    /// See https://github.com/HalFrgrd/flyline/blob/master/examples/animations.sh for more details and example usage.
    #[command(name = "create-anim", verbatim_doc_comment)]
    CreateAnim {
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
    /// Configure the colour palette.
    ///
    /// Style strings follow rich's syntax: a space-separated list of attributes
    /// (bold, dim, italic, underline, blink, reverse, strike) and colours
    /// (e.g. red, #ff0000, rgb(255,0,0), color(196)).
    ///
    /// Examples:
    ///   flyline set-color --default-theme dark
    ///   flyline set-color --default-theme auto
    ///   flyline set-color --inline-suggestion "dim italic"
    ///   flyline set-color --matching-char "bold green"
    ///   flyline set-color --default-theme light --matching-char "bold blue"
    ///   flyline set-color --recognised-command "green" --unrecognised-command "bold red"
    ///   flyline set-color --secondary-text "dim" --tutorial-hint "bold italic"
    #[command(name = "set-color", verbatim_doc_comment)]
    SetColor {
        /// Apply a built-in colour preset for dark or light terminals, or `auto` to detect
        /// the terminal background colour at startup and choose automatically.
        #[arg(long = "default-theme", value_name = "MODE")]
        default_theme: Option<settings::ColorTheme>,
        /// Style for recognised (valid) commands (e.g. "green").
        #[arg(long = "recognised-command", value_name = "STYLE")]
        recognised_command: Option<String>,
        /// Style for unrecognised (invalid) commands (e.g. "red").
        #[arg(long = "unrecognised-command", value_name = "STYLE")]
        unrecognised_command: Option<String>,
        /// Style for single-quoted strings (e.g. "yellow").
        #[arg(long = "single-quoted-text", value_name = "STYLE")]
        single_quoted_text: Option<String>,
        /// Style for double-quoted strings (e.g. "red").
        #[arg(long = "double-quoted-text", value_name = "STYLE")]
        double_quoted_text: Option<String>,
        /// Style for secondary / muted text (e.g. "dim").
        #[arg(long = "secondary-text", value_name = "STYLE")]
        secondary_text: Option<String>,
        /// Style for inline history suggestions (e.g. "dim italic", "bold red").
        #[arg(long = "inline-suggestion", value_name = "STYLE")]
        inline_suggestion: Option<String>,
        /// Style for tutorial hint text (e.g. "bold").
        #[arg(long = "tutorial-hint", value_name = "STYLE")]
        tutorial_hint: Option<String>,
        /// Style for matched characters in fuzzy-search results (e.g. "bold green").
        #[arg(long = "matching-char", value_name = "STYLE")]
        matching_char: Option<String>,
        /// Style for opening/closing bracket pairs (e.g. "bold green underline").
        #[arg(long = "opening-closing-pair", value_name = "STYLE")]
        opening_closing_pair: Option<String>,
        /// Style for normal (unstyled) text.
        #[arg(long = "normal-text", value_name = "STYLE")]
        normal_text: Option<String>,
        /// Style for shell comments (e.g. "dim italic gray").
        #[arg(long = "comment", value_name = "STYLE")]
        comment: Option<String>,
        /// Style for environment variables (e.g. "cyan").
        #[arg(long = "env-var", value_name = "STYLE")]
        env_var: Option<String>,
        /// Style for markdown H1 headings (e.g. "bold cyan").
        #[arg(long = "markdown-heading1", value_name = "STYLE")]
        markdown_heading1: Option<String>,
        /// Style for markdown H2 headings (e.g. "bold blue").
        #[arg(long = "markdown-heading2", value_name = "STYLE")]
        markdown_heading2: Option<String>,
        /// Style for markdown H3+ headings (e.g. "bold magenta").
        #[arg(long = "markdown-heading3", value_name = "STYLE")]
        markdown_heading3: Option<String>,
        /// Style for markdown inline/block code (e.g. "dim").
        #[arg(long = "markdown-code", value_name = "STYLE")]
        markdown_code: Option<String>,
    },
    /// Manage keybindings.
    ///
    /// Use 'flyline key set <KEY> <SCOPE::ACTION>' to bind a key sequence to an action.
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
    /// Tab completion is available: type 'flyline key set <KEY> <Tab>' to browse
    /// all available actions interactively.
    ///
    /// Examples:
    ///   flyline key set Ctrl+Enter normal::submit_or_newline
    ///   flyline key list
    #[command(name = "key", verbatim_doc_comment)]
    Key {
        #[command(subcommand)]
        subcommand: KeySubcommands,
    },
}

#[derive(Subcommand, Debug)]
enum KeySubcommands {
    /// Bind a key sequence to an action.
    ///
    /// KEY_SEQUENCE is a key combination such as "Ctrl+Enter" or "Alt+Left".
    /// ACTION has the form scope::action_name, e.g. "normal::submit_or_newline".
    ///
    /// Available scopes: normal, fuzzy_history_search, tab_completion,
    ///   agent_mode_waiting, agent_output_selection, agent_error,
    ///   inline_history_acceptable
    ///
    /// Examples:
    ///   flyline key set Ctrl+Enter normal::submit_or_newline
    ///   flyline key set Alt+Left normal::move_one_word_left_whitespace
    #[command(name = "set", verbatim_doc_comment, disable_help_flag = true)]
    Set {
        /// Key sequence to bind (e.g. "Ctrl+Enter", "Alt+Left").
        #[arg(num_args = 1, hide = true)]
        key_sequence: String,
        /// Action in the form scope::action_name (e.g. "normal::submit_or_newline").
        #[arg(value_parser = possible_action_names(), num_args = 1)]
        action: String,
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

// Global state for our custom input stream
static FLYLINE_INSTANCE_PTR: Mutex<Option<Box<Flyline>>> = Mutex::new(None);

// C-compatible getter function that bash will call
extern "C" fn flyline_get_char() -> c_int {
    if let Some(boxed) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_mut() {
        return boxed.get();
    }
    eprintln!("flyline_get_char: FLYLINE_INSTANCE_PTR is None");
    bash_symbols::EOF
}

// C-compatible ungetter function that bash will call
extern "C" fn flyline_unget_char(c: c_int) -> c_int {
    if let Some(boxed) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_mut() {
        return boxed.unget(c);
    }
    eprintln!("flyline_unget_char: FLYLINE_INSTANCE_PTR is None");
    c
}

extern "C" fn flyline_call_command(words: *const bash_symbols::WordList) -> c_int {
    if let Some(boxed) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_mut() {
        return boxed.call(words);
    }
    eprintln!("flyline_call_command: FLYLINE_INSTANCE_PTR is None");
    0
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

                if let Some(ref path) = parsed.dump_logs {
                    let path_opt = if path.is_empty() {
                        None
                    } else {
                        Some(std::path::PathBuf::from(path))
                    };
                    match logging::dump_logs(path_opt) {
                        Ok(path) => println!("Flyline logs dumped to {}", path.display()),
                        Err(e) => eprintln!("Failed to dump logs: {}", e),
                    }
                }

                if let Some(ref path) = parsed.stream_logs {
                    match logging::stream_logs(path.into()) {
                        Ok(path) => println!("Flyline logs streaming to {}", path.display()),
                        Err(e) => eprintln!("Failed to stream logs: {}", e),
                    }
                }

                if let Some(ref level) = parsed.log_level {
                    let filter = match level {
                        LogLevel::Error => log::LevelFilter::Error,
                        LogLevel::Warn => log::LevelFilter::Warn,
                        LogLevel::Info => log::LevelFilter::Info,
                        LogLevel::Debug => log::LevelFilter::Debug,
                        LogLevel::Trace => log::LevelFilter::Trace,
                    };
                    log::set_max_level(filter);
                }

                if let Some(path) = parsed.load_zsh_history {
                    self.settings.zsh_history_path = Some(path);
                }

                if let Some(enabled) = parsed.tutorial_mode {
                    log::info!("Tutorial mode set to {}", enabled);
                    self.settings.tutorial_mode = enabled;
                }

                if let Some(enabled) = parsed.show_animations {
                    log::info!("Animations disabled: {}", enabled);
                    self.settings.show_animations = enabled;
                }

                if let Some(enabled) = parsed.show_inline_history {
                    log::info!("Inline history suggestions set to {}", enabled);
                    self.settings.show_inline_history = enabled;
                }

                if let Some(enabled) = parsed.auto_close_chars {
                    log::info!("Auto closing char set to {}", enabled);
                    self.settings.auto_close_chars = enabled;
                }

                if let Some(mode) = parsed.use_term_emulator_cursor {
                    log::info!("Term emulator cursor mode: {:?}", mode);
                    self.settings.use_term_emulator_cursor = mode;
                }

                if let Some(enabled) = parsed.matrix_animation {
                    log::info!("Matrix animation enabled: {}", enabled);
                    self.settings.matrix_animation = enabled;
                }

                if let Some(fps) = parsed.frame_rate {
                    log::info!("Frame rate set to {}", fps);
                    self.settings.frame_rate = fps;
                }

                if let Some(mode) = parsed.mouse_mode {
                    log::info!("Mouse mode set to {:?}", mode);
                    self.settings.mouse_mode = mode;
                }

                if let Some(enabled) = parsed.send_shell_integration_codes {
                    log::info!("Shell integration codes set to {}", enabled);
                    self.settings.send_shell_integration_codes = enabled;
                }

                match parsed.command {
                    Some(Commands::AgentMode {
                        system_prompt,
                        trigger_prefix,
                        command,
                    }) => {
                        log::info!(
                            "AI command set: {:?} (trigger_prefix={:?})",
                            command,
                            trigger_prefix
                        );
                        self.settings.agent_commands.insert(
                            trigger_prefix.clone(),
                            settings::AgentModeCommand {
                                command: command.clone(),
                                system_prompt: system_prompt.clone(),
                            },
                        );
                    }
                    Some(Commands::CreateAnim {
                        name,
                        fps,
                        frames,
                        ping_pong,
                    }) => {
                        if fps <= 0.0 {
                            eprintln!(
                                "flyline create-anim: --fps must be greater than 0 (got {}); animation '{}' not registered",
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
                    Some(Commands::SetColor {
                        default_theme,
                        recognised_command,
                        unrecognised_command,
                        single_quoted_text,
                        double_quoted_text,
                        secondary_text,
                        inline_suggestion,
                        tutorial_hint,
                        matching_char,
                        opening_closing_pair,
                        normal_text,
                        comment,
                        env_var,
                        markdown_heading1,
                        markdown_heading2,
                        markdown_heading3,
                        markdown_code,
                    }) => {
                        if let Some(preset) = default_theme {
                            self.settings.color_palette.apply_theme(preset);
                            log::info!("Color theme set to {:?}", self.settings.color_theme);
                        }

                        let style_overrides: &[(
                            &Option<String>,
                            &str,
                            fn(&mut palette::Palette, Style),
                        )] = &[
                            (&recognised_command, "recognised-command", |p, s| {
                                p.recognised_command_override = Some(s)
                            }),
                            (&unrecognised_command, "unrecognised-command", |p, s| {
                                p.unrecognised_command_override = Some(s)
                            }),
                            (&single_quoted_text, "single-quoted-text", |p, s| {
                                p.single_quoted_text_override = Some(s)
                            }),
                            (&double_quoted_text, "double-quoted-text", |p, s| {
                                p.double_quoted_text_override = Some(s)
                            }),
                            (&secondary_text, "secondary-text", |p, s| {
                                p.secondary_text_override = Some(s)
                            }),
                            (&inline_suggestion, "inline-suggestion", |p, s| {
                                p.inline_suggestion_override = Some(s)
                            }),
                            (&tutorial_hint, "tutorial-hint", |p, s| {
                                p.tutorial_hint_override = Some(s)
                            }),
                            (&matching_char, "matching-char", |p, s| {
                                p.matching_char_override = Some(s)
                            }),
                            (&opening_closing_pair, "opening-closing-pair", |p, s| {
                                p.opening_and_closing_pair_override = Some(s)
                            }),
                            (&normal_text, "normal-text", |p, s| {
                                p.normal_text_override = Some(s)
                            }),
                            (&comment, "comment", |p, s| p.comment_override = Some(s)),
                            (&env_var, "env-var", |p, s| p.env_var_override = Some(s)),
                            (&markdown_heading1, "markdown-heading1", |p, s| {
                                p.markdown_heading1_override = Some(s)
                            }),
                            (&markdown_heading2, "markdown-heading2", |p, s| {
                                p.markdown_heading2_override = Some(s)
                            }),
                            (&markdown_heading3, "markdown-heading3", |p, s| {
                                p.markdown_heading3_override = Some(s)
                            }),
                            (&markdown_code, "markdown-code", |p, s| {
                                p.markdown_code_override = Some(s)
                            }),
                        ];

                        for (opt, flag_name, setter) in style_overrides {
                            if let Some(style_str) = opt {
                                match palette::parse_str_to_style(style_str) {
                                    Ok(style) => {
                                        setter(&mut self.settings.color_palette, style);
                                        log::info!("{} style set to {:?}", flag_name, style_str);
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "flyline set-color: invalid --{} style {:?}: {}",
                                            flag_name, style_str, e
                                        );
                                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                                    }
                                }
                            }
                        }
                    }
                    Some(Commands::Key { subcommand }) => match subcommand {
                        KeySubcommands::Set {
                            key_sequence,
                            action,
                        } => {
                            let binding =
                                actions::Binding::try_new_from_strs(&key_sequence, &action);
                            match binding {
                                Ok(binding) => {
                                    log::info!(
                                        "Registering key binding: {} -> {}",
                                        key_sequence,
                                        action
                                    );
                                    self.settings.keybindings.push(binding);
                                }
                                Err(e) => {
                                    eprintln!(
                                        "flyline key set: failed to parse key sequence '{}' or action '{}': {}",
                                        key_sequence, action, e
                                    );
                                    return bash_symbols::BuiltinExitCode::Usage as c_int;
                                }
                            }
                        }
                        KeySubcommands::List { key_sequence } => {
                            actions::print_bindings_table(
                                &self.settings.keybindings,
                                key_sequence.as_deref(),
                                &self.settings.key_remappings,
                            );
                        }
                        KeySubcommands::Remap { from, to } => {
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
                    },
                    None => {}
                }

                #[cfg(feature = "integration-tests")]
                if parsed.run_tab_completion_tests {
                    self.settings.run_tab_completion_tests = true;
                    println!("Running tab completion tests...");
                    app::get_command(&mut self.settings);
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
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                app::get_command(&mut self.settings)
            }));

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
                Ok(app::ExitState::WithCommand(cmd)) => cmd.into_bytes(),
                Ok(app::ExitState::EOF) => {
                    log::info!("App signaled EOF");
                    return bash_symbols::EOF;
                }
                Ok(app::ExitState::WithoutCommand) => vec![],
                Err(_) => {
                    eprintln!(
                        "flyline: app panicked; recovering with empty command. Please create an issue with the steps to reproduce at https://github.com/HalFrgrd/flyline/issues."
                    );
                    log::error!("app panicked; recovering with empty command");
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    vec![]
                }
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

fn setup_autocompletion() {
    let mut completion = Vec::new();
    generate(
        Shell::Bash,
        &mut FlylineArgs::command(),
        "flyline",
        &mut completion,
    );
    let completion_str = match std::ffi::CString::new(completion) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create completion CString: {}", e);
            return;
        }
    };
    let from_file = c"flyline_setup_autocompletion";
    #[cfg(not(feature = "pre_bash_4_4"))]
    let flags = bash_symbols::SEVAL_NOHIST | bash_symbols::SEVAL_NOOPTIMIZE;
    #[cfg(feature = "pre_bash_4_4")]
    let flags = bash_symbols::SEVAL_NOHIST;
    unsafe {
        // The called function will free the string we pass to it, so we use `xmalloc` to allocate it on the heap.
        #[cfg(not(feature = "pre_bash_4_4"))]
        bash_symbols::evalstring(
            bash_symbols::xmalloc_cstr(&completion_str),
            from_file.as_ptr(),
            flags,
        );
        #[cfg(feature = "pre_bash_4_4")]
        bash_symbols::parse_and_execute(
            bash_symbols::xmalloc_cstr(&completion_str),
            from_file.as_ptr(),
            flags,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_load(_arg: *const c_char) -> c_int {
    // Returning 0 means the load fails
    const SUCCESS: c_int = 1;
    const FAILURE: c_int = 0;

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
            logging::print_logs();
            return FAILURE;
        }
    }

    setup_autocompletion();

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
        *FLYLINE_INSTANCE_PTR.lock().unwrap() = Some(Box::new(Flyline::new()));
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
            logging::print_logs();
            return FAILURE;
        }
    }

    log::set_max_level(log::LevelFilter::Info);
    SUCCESS
}

#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_unload(_arg: *const c_char) {
    let had_instance = FLYLINE_INSTANCE_PTR.lock().unwrap().take().is_some();

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
