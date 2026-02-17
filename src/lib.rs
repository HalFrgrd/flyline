use clap::{Arg, Command as ClapCommand};
use libc::{c_char, c_int};
use std::sync::Mutex;

mod active_suggestions;
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
mod snake_animation;
mod tab_completion_context;
mod text_buffer;

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
}

impl Flyline {
    fn new() -> Self {
        Self {
            content: vec![],
            position: 0,
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

        // Parse arguments using clap
        let app = ClapCommand::new("flyline")
            .arg(
                Arg::new("version")
                    .long("version")
                    .action(clap::ArgAction::SetTrue)
                    .help("Show version information"),
            )
            .arg(
                Arg::new("disable-animations")
                    .long("disable-animations")
                    .action(clap::ArgAction::SetTrue)
                    .help("Disable animations"),
            )
            .arg(
                Arg::new("dump-logs")
                    .long("dump-logs")
                    .action(clap::ArgAction::SetTrue)
                    .help("Dump in-memory logs to file"),
            )
            .arg(
                Arg::new("stream-logs")
                    .long("stream-logs")
                    .value_name("PATH")
                    .num_args(1)
                    .help("Dump current logs to PATH and append new logs"),
            )
            .arg(
                Arg::new("log-level")
                    .long("log-level")
                    .value_name("LEVEL")
                    .num_args(1)
                    .help("Set the logging level (error, warn, info, debug, trace)"),
            );

        let args_with_prog = std::iter::once("flyline").chain(args.iter().copied());
        match app.try_get_matches_from(args_with_prog) {
            Ok(matches) => {
                log::debug!("Parsed flyline arguments: {:?}", matches);

                if matches.get_flag("version") {
                    println!("flyline version {}", env!("CARGO_PKG_VERSION"));
                }

                if matches.get_flag("disable-animations") {
                    log::info!("Animations disabled");
                    // TODO: Set animation flag or pass to app
                }

                if matches.get_flag("dump-logs") {
                    match logging::dump_logs() {
                        Ok(path) => println!("Flyline logs dumped to {}", path.display()),
                        Err(e) => eprintln!("Failed to dump logs: {}", e),
                    }
                }

                if let Some(path) = matches.get_one::<String>("stream-logs") {
                    match logging::stream_logs(path.into()) {
                        Ok(path) => println!("Flyline logs streaming to {}", path.display()),
                        Err(e) => eprintln!("Failed to stream logs: {}", e),
                    }
                }

                if let Some(level) = matches.get_one::<String>("log-level") {
                    match level.as_str() {
                        "error" => log::set_max_level(log::LevelFilter::Error),
                        "warn" => log::set_max_level(log::LevelFilter::Warn),
                        "info" => log::set_max_level(log::LevelFilter::Info),
                        "debug" => log::set_max_level(log::LevelFilter::Debug),
                        "trace" => log::set_max_level(log::LevelFilter::Trace),
                        _ => eprintln!("Invalid log level: {}", level),
                    }
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

            self.content = match app::get_command() {
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
        c"longer docs here".as_ptr() as *const c_char,
        c"more help here".as_ptr() as *const c_char,
        ::std::ptr::null(),
    ])
        .as_ptr(),
    short_doc: c"flyline: advanced command line interface for bash".as_ptr() as *const c_char,
    handle: std::ptr::null(),
};

#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_load(_arg: *const c_char) -> c_int {
    // Returning 0 means the load fails

    // TODO: panic catch
    unsafe {
        if bash_symbols::interactive_shell == 0 {
            // Not an interactive shell, do nothing
            return 1;
        }
    }

    logging::init().unwrap_or_else(|e| {
        eprintln!("Flyline failed to setup logging: {}", e);
    });

    // This is how we ensure that our custom input stream is used by bash instead of readline.
    // This code is run during `run_startup_files` so we can't modify bash_input directly.
    // `bash_input` is being used to read the rc files at this point. set_bash_input() has yet to be called.
    // `stream_list` contains only a sentinel input stream at this point.
    // Normally when it is popped off the list after rc files are read, readline stdin is added since
    // `with_input_from_stdin` sees that the current bash_input is of type st_stdin.
    // So we modify the sentinel node before that happens so that in set_bash_input,
    // with_input_from_stdin will see that the current bash_input is fit for purpose and not add readline stdin.

    let setup_bash_input = |bash_input: *mut bash_symbols::BashInput| {
        // Allocate the name string on the heap using libc
        // Bash expects name to be heap allocated so it can free it later
        let name_bytes = b"flyline_input\0";
        let name_ptr = unsafe {
            let ptr = libc::malloc(name_bytes.len()) as *mut libc::c_char;
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    name_bytes.as_ptr(),
                    ptr as *mut u8,
                    name_bytes.len(),
                );
            }
            ptr
        };

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
        if !bash_symbols::stream_list.is_null() {
            let stream_list_head: &mut bash_symbols::StreamSaver = &mut *bash_symbols::stream_list;
            let next_is_null = stream_list_head.next.is_null();
            if next_is_null {
                // No streams in the list, we can set ours
                // and then with_input_from_stdin won't add readline
                // stream_on_stack (st_stdin) will be true.
                // This basically takes over the sentinel node at the base of the stream_list
                log::info!("Setting flyline input stream at the head of the list");
                setup_bash_input(&mut stream_list_head.bash_input);
            } else {
                log::error!("stream_list has more than one entry, cannot set flyline input stream");
            }
        } else {
            // This is so that we can load it on the repl after startup
            log::warn!("stream_list is null, seeing if we can set flyline input stream");

            if !bash_symbols::bash_input.name.is_null() {
                log::info!("Setting flyline input stream via bash_input");

                let current_input_name =
                    std::ffi::CStr::from_ptr(bash_symbols::bash_input.name).to_string_lossy();

                if current_input_name.starts_with("readline") {
                    log::info!("bash_input.name is readline, safe to override");
                    bash_symbols::push_stream(0);
                    setup_bash_input(&raw mut bash_symbols::bash_input);
                } else {
                    log::warn!(
                        "bash_input.name is '{}', not overriding anyway",
                        current_input_name
                    );
                }
            } else {
                log::error!("bash_input.name is null, cannot set flyline input stream");
            }
        }
    }

    1
}

#[unsafe(no_mangle)]
pub extern "C" fn flyline_builtin_unload(_arg: *const c_char) {
    *FLYLINE_INSTANCE_PTR.lock().unwrap() = None;

    unsafe {
        if bash_symbols::stream_list.is_null() {
            println!("stream_list is null, trying to setup readline");

            // we don't have access to yy_readline_(un)get so we can't set it directly
            // but we can call with_input_from_stdin which will set it up properly
            bash_symbols::bash_input.stream_type = bash_symbols::StreamType::StNone;
            bash_symbols::with_input_from_stdin();
        } else {
            let head: &mut bash_symbols::StreamSaver = &mut *bash_symbols::stream_list;
            let current_input_name =
                std::ffi::CStr::from_ptr(head.bash_input.name).to_string_lossy();
            println!(
                "Found stream_list entry with name: {} and type: {:?}",
                current_input_name, head.bash_input.stream_type
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
