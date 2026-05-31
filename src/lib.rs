use libc::{c_char, c_int};
use std::sync::Mutex;

#[cfg(feature = "pre_bash_4_4")]
use ctor::ctor;

#[macro_use]
mod macros;
mod active_suggestions;
mod agent_mode;
mod app;
mod bash_funcs;
mod bash_symbols;
mod cli;
mod command_acceptance;
mod content_builder;
mod content_utils;
mod cursor;
mod dparser;
mod globbing;
mod history;
pub mod hostnames;
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
pub(crate) struct Flyline {
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

            self.settings.last_app_closed_at = Some(std::time::Instant::now());

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
        let name_ptr = unsafe { bash_symbols::xmalloc_cstr(name) };
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
