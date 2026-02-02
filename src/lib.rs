use bash_builtins::{Args, Builtin, BuiltinOptions, Result, builtin_metadata};
use libc::{c_char, c_int, c_uint};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

mod active_suggestions;
mod app;
mod bash_env_manager;
mod bash_funcs;
mod bash_symbols;
mod command_acceptance;
mod content_builder;
mod cursor_animation;
mod history;
mod iter_first_last;
mod palette;
mod prompt_manager;
mod snake_animation;
mod tab_completion_context;
mod text_buffer;

// Global state for our custom input stream
static FLYLINE_INSTANCE_PTR: Mutex<Option<Arc<Mutex<Flyline>>>> = Mutex::new(None); // TODO: do I need Mutex optoin arc mutex??

// C-compatible getter function that bash will call
extern "C" fn flyline_get_char() -> c_int {
    if let Some(arc) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_ref() {
        if let Ok(mut stream) = arc.lock() {
            return stream.get();
        }
    }
    bash_symbols::EOF
}

// C-compatible ungetter function that bash will call
extern "C" fn flyline_unget_char(c: c_int) -> c_int {
    // log::debug!(
    //     "Calling flyline_unget with char: {} (asci={})",
    //     c,
    //     c as u8 as char
    // );
    if let Some(arc) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_ref() {
        if let Ok(mut stream) = arc.lock() {
            return stream.unget(c);
        }
    }
    c
}

extern "C" fn flyline_call_command(list: *mut bash_symbols::WordList) -> c_int {
    if let Some(arc) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_ref() {
        if let Ok(mut stream) = arc.lock() {
            return stream.call(list);
        }
    }
    0
}

fn flyline_setup_logging() -> Result<()> {
    let home_dir = bash_builtins::variables::find_as_string("HOME")
        .as_ref()
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        .unwrap_or("/tmp/".to_string());
    let log_file_path = PathBuf::from(home_dir).join("flyline.logs");

    // Initialize simplelog to write to file with file and line number information
    use simplelog::*;
    let log_file = std::fs::File::create(&log_file_path)?;

    WriteLogger::init(
        LevelFilter::Debug,
        ConfigBuilder::new()
            .set_time_format_rfc3339()
            .set_target_level(LevelFilter::Off)
            .set_location_level(LevelFilter::Debug)
            .add_filter_ignore_str("flyline::text_buffer")
            .add_filter_ignore_str("flyline::tab_completion")
            // .add_filter_ignore_str("flyline::history")
            .add_filter_ignore_str("flyline::bash_funcs")
            .build(),
        log_file,
    )?;

    log::info!(
        "Flyline logging initialized, output will be logged to: {}",
        log_file_path.display()
    );

    Ok(())
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

    fn call(&mut self, list: *mut bash_symbols::WordList) -> c_int {
        let mut args = vec![];
        unsafe {
            let mut current = list;
            while !current.is_null() {
                let word = &*(*current).word;
                let c_str = std::ffi::CStr::from_ptr(word.word);
                if let Ok(str_slice) = c_str.to_str() {
                    args.push(str_slice.to_string());
                }
                current = (*current).next;
            }
        }

        log::debug!("flyline called with args: {:?}", args);
        0

        // if args.is_empty() {
        //     return Err(bash_builtins::Error::Usage);
        // }

        // for opt in args.options() {
        //     match opt? {
        //         Opt::Status => {
        //             // Iterate through the stream_list linked list and print each entry
        //             unsafe {
        //                 let mut current = bash_symbols::stream_list;
        //                 let mut index = 0;

        //                 println!("=== Stream List ===");
        //                 while !current.is_null() {
        //                     let stream = &*current;

        //                     let name = if stream.bash_input.name.is_null() {
        //                         "null".to_string()
        //                     } else {
        //                         std::ffi::CStr::from_ptr(stream.bash_input.name)
        //                             .to_string_lossy()
        //                             .into_owned()
        //                     };

        //                     let stream_type = match stream.bash_input.stream_type {
        //                         bash_symbols::StreamType::StNone => "st_none",
        //                         bash_symbols::StreamType::StStdin => "st_stdin",
        //                         bash_symbols::StreamType::StStream => "st_stream",
        //                         bash_symbols::StreamType::StString => "st_string",
        //                         bash_symbols::StreamType::StBStream => "st_bstream",
        //                     };

        //                     println!("[{}] name: '{}', type: {}", index, name, stream_type);

        //                     current = stream.next;
        //                     index += 1;
        //                 }
        //                 println!("===================");
        //             }
        //         }
        //         Opt::SetKeyBinding(binding) => {
        //             println!("Not yet implemented: {}", binding);
        //         }
        //         Opt::Version => {
        //             println!("flyline version {}", env!("CARGO_PKG_VERSION"));
        //         }
        //     }
        // }

        // // It is an error if we receive free arguments.
        // args.finished()?;

        // Ok(())
        // return 0;
    }

    fn get(&mut self) -> c_int {
        // log::debug!("Getting byte from flyline input stream");
        if self.content.is_empty() || self.position >= self.content.len() {
            log::debug!("Input stream is empty or at end, fetching new command");
            // log::debug!(
            //     "self.content.len() = {}, self.position = {}",
            //     self.content.len(),
            //     self.position
            // );
            // for b in &self.content {
            //     log::debug!("Existing content byte: {} (asci={})", b, *b as char);
            // }

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

static NAME: &[u8] = b"flyline\0";

/* Documentation strings */
static SHORT_DOC: &[u8] = b"hello [arg]\0";

static LONG_DOC: &[&[u8]] = &[b"hello: print a greeting from Rust\0", b"\0"];

/* Exported builtin struct */
#[no_mangle]
pub static mut flyline_struct: bash_symbols::BashBuiltin = bash_symbols::BashBuiltin {
    name: NAME.as_ptr(),
    function: Some(flyline_call_command),
    flags: bash_symbols::BUILTIN_ENABLED,
    long_doc: LONG_DOC.as_ptr() as *const *const c_char,
    short_doc: SHORT_DOC.as_ptr() as *const c_char,
    handle: std::ptr::null_mut(),
};

/// Called when the builtin is loaded
#[no_mangle]
pub extern "C" fn flyline_init(_struct: *mut bash_symbols::BashBuiltin) -> c_int {
    println!("flyline builtin initialized");
    // TODO: panic catch
    unsafe {
        if bash_symbols::interactive_shell == 0 {
            // Not an interactive shell, do nothing
            return 0;
        }
    }

    flyline_setup_logging().unwrap_or_else(|e| {
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
                let name = std::ffi::CString::new("flyline_input").unwrap();

                stream_list_head.bash_input.stream_type = bash_symbols::StreamType::StStdin;
                stream_list_head.bash_input.name = name.as_ptr() as *mut i8;
                stream_list_head.bash_input.getter = Some(flyline_get_char);
                stream_list_head.bash_input.ungetter = Some(flyline_unget_char);

                std::mem::forget(name);

                // Store the Arc globally so C callbacks can access it
                *FLYLINE_INSTANCE_PTR.lock().unwrap() = Some(Arc::new(Mutex::new(Flyline::new())));
            } else {
                log::error!("stream_list has more than one entry, cannot set flyline input stream");
            }
        } else {
            log::warn!("stream_list is null, seeing if we can set flyline input stream");

            if !bash_symbols::bash_input.name.is_null() {
                log::info!("Setting flyline input stream via bash_input");

                let current_input =
                    std::ffi::CStr::from_ptr(bash_symbols::bash_input.name).to_string_lossy();

                if current_input.starts_with("readline") {
                    log::info!("bash_input.name is readline, safe to override");
                    let name = std::ffi::CString::new("flyline_input").unwrap();

                    bash_symbols::push_stream(0);
                    bash_symbols::bash_input.stream_type = bash_symbols::StreamType::StStdin;
                    bash_symbols::bash_input.name = name.as_ptr() as *mut i8;
                    bash_symbols::bash_input.getter = Some(flyline_get_char);
                    bash_symbols::bash_input.ungetter = Some(flyline_unget_char);

                    std::mem::forget(name);

                    // Store the Arc globally so C callbacks can access it
                    *FLYLINE_INSTANCE_PTR.lock().unwrap() =
                        Some(Arc::new(Mutex::new(Flyline::new())));
                } else {
                    log::warn!(
                        "bash_input.name is '{}', not overriding anyway",
                        current_input
                    );
                }
            } else {
                log::error!("bash_input.name is null, cannot set flyline input stream");
            }
        }
    }

    0
}

/// Called when the builtin is unloaded
#[no_mangle]
pub extern "C" fn flyline_deinit(_struct: *mut bash_symbols::BashBuiltin) -> c_int {
    println!("flyline builtin deinitialized");
    0
}
