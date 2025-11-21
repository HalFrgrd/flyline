//! Bash builtin with custom input stream from Rust.

use bash_builtins::{Args, Builtin, BuiltinOptions, Result, builtin_metadata};
use std::io::{Write, stdout};
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

mod app;
mod cursor_animation;
mod events;

// Bash input stream types from bash's input.h
#[repr(C)]
#[allow(dead_code)]
enum StreamType {
    StNone = 0,
    StStdin = 1,
    StStream = 2,
    StString = 3,
    StBStream = 4,
}

// INPUT_STREAM union from bash
#[repr(C)]
union InputStreamLocation {
    string: *mut c_char,
    _file: *mut libc::c_void, // FILE* - we don't use this
    _buffered_fd: c_int,      // for st_bstream - we don't use this
}

// External bash_input symbol that bash provides
unsafe extern "C" {
    fn init_yy_io(
        get: extern "C" fn() -> c_int,
        unget: extern "C" fn(c_int) -> c_int,
        type_: StreamType,
        name: *const c_char,
        location: InputStreamLocation,
    );
}

// Global state for our custom input stream
static JOBU_INPUT: Mutex<Option<JobuInputStream>> = Mutex::new(None);

struct JobuInputStream {
    content: Vec<u8>,
    position: usize,
}

const EOF: c_int = -1;

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
        log::debug!("Getting byte from jobu input stream");
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
            // std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if self.position < self.content.len() {
            let byte = self.content[self.position];
            self.position += 1;
            log::debug!("Returning byte: {} (asci={})", byte, byte as char);
            byte as c_int
        } else {
            log::debug!("End of input stream reached, returning EOF");
            EOF
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
    let mut stream = JOBU_INPUT.lock().unwrap();
    if let Some(ref mut s) = *stream {
        s.get()
    } else {
        // shoudl never be here
        EOF // EOF if no stream is set
    }
}

// C-compatible ungetter function that bash will call
extern "C" fn jobu_unget(c: c_int) -> c_int {
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
    #[opt = 's']
    Set,
}

#[derive(Default)]
struct Jobu();

impl Builtin for Jobu {
    fn call(&mut self, args: &mut Args) -> Result<()> {
        setup_logging().unwrap_or_else(|e| {
            eprintln!("Failed to setup logging: {}", e);
        });

        // No options: print the current value and increment it.
        if args.is_empty() {
            return Err(bash_builtins::Error::Usage);
        }

        for opt in args.options() {
            match opt? {
                Opt::Set => {
                    // Set the custom input stream for bash
                    let mut stream = JOBU_INPUT.lock().unwrap();
                    *stream = Some(JobuInputStream::new());

                    unsafe {
                        // Create a C string for the name
                        let name = std::ffi::CString::new("jobu_input").unwrap();

                        // Create empty location - we don't use it since we have custom getters
                        let location = InputStreamLocation {
                            string: std::ptr::null_mut(),
                        };

                        // Initialize bash's input system with our custom getters
                        init_yy_io(
                            jobu_get,
                            jobu_unget,
                            StreamType::StStdin,
                            name.as_ptr(),
                            location,
                        );

                        // Keep the name alive by leaking it (bash will use it)
                        std::mem::forget(name);
                    }
                    writeln!(stdout(), "Input stream set to jobu")?;
                }
            }
        }

        // It is an error if we receive free arguments.
        args.finished()?;

        Ok(())
    }
}
