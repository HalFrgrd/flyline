use bash_builtins::{Args, Builtin, BuiltinOptions, Result, builtin_metadata};
use std::env;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

mod active_suggestions;
mod app;
mod bash_funcs;
mod bash_symbols;
mod command_acceptance;
mod content_builder;
mod cursor_animation;
mod events;
mod flash_testing;
mod history;
mod iter_first_last;
mod lexer;
mod prompt_manager;
mod snake_animation;
mod tab_completion;
mod text_buffer;

// Global state for our custom input stream
static FLYLINE_INSTANCE_PTR: Mutex<Option<Arc<Mutex<Flyline>>>> = Mutex::new(None);

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

fn setup_logging() -> Result<()> {
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
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
    history: history::HistoryManager,
    cached_content_during_resize: String,
}

impl Flyline {
    fn new() -> Self {
        Self {
            content: vec![],
            position: 0,
            history: history::HistoryManager::new(),
            cached_content_during_resize: "".to_string(),
        }
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

            let starting_content = self.cached_content_during_resize.clone();
            self.cached_content_during_resize = "".to_string();
            self.content = match app::get_command(&mut self.history, starting_content) {
                app::AppRunningState::ExitingWithCommand(cmd) => {
                    let timestamp: Option<u64> = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs());
                    self.history.add_entry(timestamp, &cmd);
                    cmd.into_bytes()
                }
                app::AppRunningState::ExitingForResize(buf) => {
                    self.cached_content_during_resize = buf;
                    vec![]
                }
                app::AppRunningState::Running => vec![],
                app::AppRunningState::ExitingWithoutCommand => vec![],
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

struct FlylineSentinel;

impl Default for FlylineSentinel {
    fn default() -> Self {
        setup_logging().unwrap_or_else(|e| {
            eprintln!("Failed to setup logging: {}", e);
        });

        // TODO: should I try another approach for any reason?
        // like cehcking if hte current bash_input is readline, and replace the name and getters?

        // TODO: should check interactive or interactive_shell?

        // This is a hacky way to ensure that our custom input stream is used by bash.
        // This code is run during `run_startup_files` so we cant modify bash_input directly.
        // bash_input is being used to read the rc files at this point.
        // set_bash_input() has yet to be called.
        // stream_list contains only a sentinel input stream at this point.
        // normally when it it popped off the list after rc files are read, readline stdin is added
        // since with_input_from_stdin sees that the current bash_input is not good.
        // So we modify the sentinel node before that happens so that in set_bash_input,
        // with_input_from_stdin will see that the current bash_input is fit for purpose and not add readline stdin.

        unsafe {
            let stream_list_head = &mut *bash_symbols::stream_list;
            let stream_is_null = bash_symbols::stream_list.is_null();
            // println!("stream_list is null: {}", stream_is_null);
            if !stream_is_null && bash_symbols::interactive_shell != 0 {
                let next_is_null = stream_list_head.next.is_null();
                // println!("stream_list.next is null: {}", next_is_null);
                if next_is_null {
                    // No streams in the list, we can set ours
                    // and then with_input_from_stdin won't add readline
                    // stream_on_stack (st_stdin) will be true.
                    // This basically takes over the sentinel node at the base of the stream_list
                    println!("Setting flyline input stream at the head of the list");
                    let name = std::ffi::CString::new("flyline_input").unwrap();

                    stream_list_head.bash_input.type_ = bash_symbols::StreamType::StStdin;
                    stream_list_head.bash_input.name = name.as_ptr() as *mut i8;
                    stream_list_head.bash_input.getter = Some(flyline_get_char);
                    stream_list_head.bash_input.ungetter = Some(flyline_unget_char);

                    std::mem::forget(name);
                } else {
                    log::error!(
                        "stream_list has more than one entry, cannot set flyline input stream"
                    );
                }
            } else {
                log::error!(
                    "{:?} {:?} cannot set flyline input stream",
                    stream_is_null,
                    bash_symbols::interactive_shell
                );
            }
        }

        // Store the Arc globally so C callbacks can access it
        *FLYLINE_INSTANCE_PTR.lock().unwrap() = Some(Arc::new(Mutex::new(Flyline::new())));
        FlylineSentinel {}
    }
}

#[derive(BuiltinOptions)]
enum Opt {
    #[opt = 'r']
    Read,
    #[opt = 's']
    SetKeyBinding(String),
    // #[opt = 'h']
}

impl Builtin for FlylineSentinel {
    fn call(&mut self, args: &mut Args) -> Result<()> {
        // let _state = __bash_builtin__state_flyline().lock().unwrap();

        // No options: print the current value and increment it.
        if args.is_empty() {
            return Err(bash_builtins::Error::Usage);
        }

        for opt in args.options() {
            match opt? {
                Opt::Read => {
                    // Iterate through the stream_list linked list and print each entry
                    unsafe {
                        let mut current = bash_symbols::stream_list;
                        let mut index = 0;

                        println!("=== Stream List ===");
                        while !current.is_null() {
                            let stream = &*current;

                            let name = if stream.bash_input.name.is_null() {
                                "null".to_string()
                            } else {
                                std::ffi::CStr::from_ptr(stream.bash_input.name)
                                    .to_string_lossy()
                                    .into_owned()
                            };

                            let stream_type = match stream.bash_input.type_ {
                                bash_symbols::StreamType::StNone => "st_none",
                                bash_symbols::StreamType::StStdin => "st_stdin",
                                bash_symbols::StreamType::StStream => "st_stream",
                                bash_symbols::StreamType::StString => "st_string",
                                bash_symbols::StreamType::StBStream => "st_bstream",
                            };

                            println!("[{}] name: '{}', type: {}", index, name, stream_type);

                            current = stream.next;
                            index += 1;
                        }
                        println!("===================");
                    }
                }
                Opt::SetKeyBinding(binding) => {
                    println!("Setting key binding for flyline: {}", binding);
                    // if let Some(arc) = FLYLINE_INSTANCE_PTR.lock().unwrap().as_ref() {
                    //     if let Ok(mut flyline) = arc.lock() {
                    //         flyline.new_setting(&binding);
                    //     }
                    // }
                }
            }
        }

        // It is an error if we receive free arguments.
        args.finished()?;

        Ok(())
    }
}

builtin_metadata!(
    name = "flyline",
    create = FlylineSentinel::default,
    short_doc = "Set flyline as a custom input stream for bash.",
    long_doc = "
        Set flyline as a custom input stream for bash.
    ",
);
