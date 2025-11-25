use bash_builtins::{Args, Builtin, BuiltinOptions, Result, builtin_metadata};
use std::os::raw::c_int;
use std::sync::Mutex;

mod app;
mod bash_funcs;
mod bash_symbols;
mod cursor_animation;
mod events;
mod layout_manager;
mod snake_animation;

// Global state for our custom input stream
static JOBU_INPUT: Mutex<Option<JobuInputStream>> = Mutex::new(None);

struct JobuInputStream {
    content: Vec<u8>,
    position: usize,
}

fn setup_logging() -> Result<()> {
    use std::env;
    use std::path::PathBuf;

    // Get home directory
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_file_path = PathBuf::from(home_dir).join("jobu.logs");

    // Initialize simple-logging to write to file
    simple_logging::log_to_file(&log_file_path, log::LevelFilter::Debug)?;

    log::info!(
        "Jobu logging initialized, output will be logged to: {}",
        log_file_path.display()
    );

    Ok(())
}

impl JobuInputStream {
    fn new() -> Self {
        Self {
            content: vec![],
            position: 0,
        }
    }

    fn get(&mut self) -> c_int {
        // log::debug!("Getting byte from jobu input stream");
        if self.content.is_empty() || self.position >= self.content.len() {
            log::debug!("Input stream is empty or at end, fetching new command");

            const PS1_VAR_NAME: &str = "PS1";
            let ps1_prompt = bash_builtins::variables::find_as_string(PS1_VAR_NAME)
                .as_ref()
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
                .unwrap_or("default> ".into());

            self.content = app::get_command(ps1_prompt).into_bytes();
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

// C-compatible getter function that bash will call
extern "C" fn jobu_get() -> c_int {
    // log::debug!("Calling jobu_get");
    let mut stream = JOBU_INPUT.lock().unwrap();
    if let Some(ref mut s) = *stream {
        s.get()
    } else {
        // shoudl never be here
        bash_symbols::EOF // EOF if no stream is set
    }
}

// C-compatible ungetter function that bash will call
extern "C" fn jobu_unget(c: c_int) -> c_int {
    // log::debug!(
    //     "Calling jobu_unget with char: {} (asci={})",
    //     c,
    //     c as u8 as char
    // );
    let mut stream = JOBU_INPUT.lock().unwrap();
    if let Some(ref mut s) = *stream {
        s.unget(c)
    } else {
        c
    }
}

builtin_metadata!(
    name = "jobu",
    create = Jobu::default,
    short_doc = "Set jobu as a custom input stream for bash.",
    long_doc = "
        Set jobu as a custom input stream for bash.
    ",
);

#[derive(BuiltinOptions)]
enum Opt {
    #[opt = 'r']
    Read,

}

struct Jobu();

impl Default for Jobu {
    fn default() -> Self {
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
            println!(
                "interactive_shell in default={}",
                bash_symbols::interactive_shell
            );
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
                    println!("Setting jobu input stream at the head of the list");
                    let name = std::ffi::CString::new("jobu_input").unwrap();
                    // let location = bash_symbols::InputStreamLocation {
                    //     string: std::ffi::CString::new("sdflkjsdf").unwrap().into_raw(),
                    // };
                    stream_list_head.bash_input.type_ = bash_symbols::StreamType::StStdin;
                    stream_list_head.bash_input.name = name.as_ptr() as *mut i8;
                    // stream_list_head.bash_input.location = location;
                    stream_list_head.bash_input.getter = Some(jobu_get);
                    stream_list_head.bash_input.ungetter = Some(jobu_unget);

                    // std::mem::forget(location);
                    std::mem::forget(name);

                    let mut stream = JOBU_INPUT.lock().unwrap();
                    *stream = Some(JobuInputStream::new());
                } else {
                    log::error!(
                        "stream_list has more than one entry, cannot set jobu input stream"
                    );
                }
            } else {
                log::error!(
                    "{:?} {:?} cannot set jobu input stream",
                    stream_is_null,
                    bash_symbols::interactive_shell
                );
            }
        }

        // let stream_saver_head = unsafe { &mut *bash_symbols::stream_list };
        // let name = unsafe {
        //     if stream_saver_head.bash_input.name.is_null() {
        //         "null".to_string()
        //     } else {
        //         std::ffi::CStr::from_ptr(stream_saver_head.bash_input.name)
        //             .to_string_lossy()
        //             .into_owned()
        //     }
        // };
        // let stream_head_is_readline = name == "readline stdin";

        // println!("stream_list: {:?}, is_readline: {:?}", name, stream_head_is_readline);
        // if stream_head_is_readline {

        //     unsafe {
        //         let new_name = b"jobujobu";
        //         for i in 0..8 {
        //             *stream_saver_head.bash_input.name.offset(i) = new_name[i as usize] as i8;
        //         }
        //     }
        //     // Set the custom input stream for bash
        // let mut stream = JOBU_INPUT.lock().unwrap();
        // *stream = Some(JobuInputStream::new());

        //     unsafe {
        //         // Create a C string for the name
        //         // let name = std::ffi::CString::new("jobu_input").unwrap();

        //         // Create empty location - we don't use it since we have custom getters
        //         // let location = bash_symbols::InputStreamLocation {
        //         //     string: std::ptr::null_mut(),
        //         // };

        //         // // Initialize bash's input system with our custom getters
        //         // bash_symbols::init_yy_io(
        //         //     jobu_get,
        //         //     jobu_unget,
        //         //     bash_symbols::StreamType::StStdin,
        //         //     name.as_ptr(),
        //         //     location,
        //         // );
        //         stream_saver_head.bash_input.getter = Some(jobu_get);
        //         stream_saver_head.bash_input.ungetter = Some(jobu_unget);

        //         // Keep the name alive by leaking it (bash will use it)
        //         // std::mem::forget(name);
        //     }
        //     writeln!(stdout(), "Input stream set to jobu")?;

        // }

        Jobu()
    }
}

impl Builtin for Jobu {
    fn call(&mut self, args: &mut Args) -> Result<()> {
        // let _state = __bash_builtin__state_jobu().lock().unwrap();

        setup_logging().unwrap_or_else(|e| {
            eprintln!("Failed to setup logging: {}", e);
        });

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
            }
        }

        // It is an error if we receive free arguments.
        args.finished()?;

        Ok(())
    }
}
