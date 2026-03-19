use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use libc::{c_char, c_int};
use std::sync::Mutex;

mod active_suggestions;
mod ai_command;
mod app;
mod bash_env_manager;
mod bash_funcs;
mod bash_symbols;
mod command_acceptance;
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
mod snake_animation;
mod tab_completion_context;
mod text_buffer;

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
#[command(name = "flyline", styles = get_styles())]
struct FlylineArgs {
    /// Show version information
    #[arg(long)]
    version: bool,
    /// Disable animations
    #[arg(long = "disable-animations")]
    disable_animations: bool,
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
    /// Load zsh history in addition to bash history
    #[arg(long = "load-zsh-history")]
    load_zsh_history: bool,
    /// Enable or disable tutorial mode with hints for first-time users.
    /// Use `--tutorial-mode false` to disable.
    #[arg(long = "tutorial-mode", default_missing_value = "true", num_args = 0..=1)]
    tutorial_mode: Option<bool>,
    /// Disable automatic closing character insertion (e.g. do not insert `)` after `(`)
    #[arg(long = "disable-auto-closing-char")]
    disable_auto_closing_char: bool,
    /// Mouse capture mode (none, simple, smart). Default is smart.
    #[arg(long = "mouse-mode", value_name = "MODE")]
    mouse_mode: Option<settings::MouseMode>,
    /// Command (and arguments) used for AI mode. The current buffer is appended as the final
    /// argument when Ctrl+I is pressed. Example: `flyline --ai-command llm prompt`
    #[arg(long = "ai-command", num_args = 1.., allow_hyphen_values = true)]
    ai_command: Vec<String>,
    // Only for integration tests
    #[cfg(feature = "integration-tests")]
    #[arg(long = "run-tab-completion-tests")]
    run_tab_completion_tests: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a custom prompt animation.
    ///
    /// Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
    /// with the current animation frame on every render.  Frames may include
    /// ANSI colour sequences written as `\e` (e.g. `\e[33m`).
    ///
    /// Example:
    ///   flyline create-anim --name COOL_SPINNER --fps 10 '\e[33m|' '/' '-' '\\'
    #[command(name = "create-anim")]
    CreateAnim {
        /// Name to embed in prompt strings as the animation placeholder.
        #[arg(long)]
        name: String,
        /// Playback speed in frames per second (default: 10).
        #[arg(long, default_value = "10")]
        fps: f64,
        /// One or more animation frames (positional).  Use `\e` for the ESC character.
        frames: Vec<String>,
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

        // If no args were provided, or --help/-h was requested, print help
        if args.is_empty() || args.iter().any(|a| *a == "--help" || *a == "-h") {
            FlylineArgs::command().print_help().ok();
            println!();
            return bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int;
        }

        // args contains words from WordList; first word is not the command name unlike argv
        let args_with_prog = std::iter::once("flyline").chain(args.iter().copied());
        match FlylineArgs::try_parse_from(args_with_prog) {
            Ok(parsed) => {
                log::debug!("Parsed flyline arguments: {:?}", parsed);

                if parsed.version {
                    println!(
                        "flyline version {} ({})",
                        env!("CARGO_PKG_VERSION"),
                        if cfg!(debug_assertions) {
                            "debug"
                        } else {
                            "release"
                        }
                    );
                }

                if parsed.disable_animations {
                    log::info!("Animations disabled");
                    self.settings.disable_animations = true;
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

                if parsed.load_zsh_history {
                    self.settings.load_zsh_history = true;
                }

                if let Some(enabled) = parsed.tutorial_mode {
                    log::info!("Tutorial mode set to {}", enabled);
                    self.settings.tutorial_mode = enabled;
                }

                if parsed.disable_auto_closing_char {
                    log::info!("Auto closing char disabled");
                    self.settings.disable_auto_closing_char = true;
                }

                if let Some(mode) = parsed.mouse_mode {
                    log::info!("Mouse mode set to {:?}", mode);
                    self.settings.mouse_mode = mode;
                }

                if !parsed.ai_command.is_empty() {
                    log::info!("AI command set: {:?}", parsed.ai_command);
                    self.settings.ai_command = parsed.ai_command;
                }

                if let Some(Commands::CreateAnim { name, fps, frames }) = parsed.command {
                    if fps <= 0.0 {
                        eprintln!(
                            "flyline create-anim: --fps must be greater than 0 (got {}); animation '{}' not registered",
                            fps, name
                        );
                        return bash_symbols::BuiltinExitCode::Usage as c_int;
                    }
                    log::info!(
                        "Registering animation '{}' at {} fps with {} frame(s)",
                        name,
                        fps,
                        frames.len()
                    );
                    self.settings
                        .custom_animations
                        .push(settings::PromptAnimation { name, fps, frames });
                }

                #[cfg(feature = "integration-tests")]
                if parsed.run_tab_completion_tests {
                    self.settings.run_tab_completion_tests = true;
                    println!("Running tab completion tests...");
                    app::get_command(&self.settings);
                    println!("Finished running tab completion tests.");
                }

                bash_symbols::BuiltinExitCode::ExecutionSuccess as c_int
            }
            Err(e) => {
                eprintln!("Error parsing arguments: {}", e);
                return bash_symbols::BuiltinExitCode::Usage as c_int;
            }
        }
    }

    fn get(&mut self) -> c_int {
        // log::debug!("Getting byte from flyline input stream");
        if self.content.is_empty() || self.position >= self.content.len() {
            log::debug!("---------------------- Starting app ------------------------");

            self.content = match app::get_command(&self.settings) {
                app::ExitState::WithCommand(cmd) => cmd.into_bytes(),
                app::ExitState::WithoutCommand => vec![],
            };
            log::debug!("---------------------- App finished ------------------------");
            self.content.push(b'\n');
            self.position = 0;
        }

        if self.position < self.content.len() {
            let byte = self.content[self.position];
            self.position += 1;
            // log::debug!("Returning byte: {} (asci={})", byte, byte as char);
            byte as c_int
        } else {
            log::debug!("End of input stream reached, returning EOF");
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
    long_doc: (&[
        c"Refer to `flyline --help` for more help.".as_ptr() as *const c_char,
        ::std::ptr::null(),
    ])
        .as_ptr(),
    short_doc: c"flyline: advanced command line editing for bash.".as_ptr() as *const c_char,
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
    let flags = bash_symbols::SEVAL_NOHIST | bash_symbols::SEVAL_NOOPTIMIZE;
    unsafe {
        bash_symbols::evalstring(completion_str.into_raw(), from_file.as_ptr(), flags);
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

    // TODO: panic catch
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
        // Default global allocator uses libc alloc which should be the same as what bash uses
        let name = std::ffi::CString::new("flyline").unwrap();
        let name_ptr = name.into_raw();
        unsafe {
            (*bash_input).stream_type = bash_symbols::StreamType::StStdin;
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
                if stream.bash_input.stream_type == bash_symbols::StreamType::StStdin
                    || stream.bash_input.stream_type == bash_symbols::StreamType::StNone
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
            bash_symbols::bash_input.stream_type = bash_symbols::StreamType::StNone;
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
