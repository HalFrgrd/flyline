//! Bash builtin with custom input stream from Rust.

use bash_builtins::{builtin_metadata, Args, Builtin, BuiltinOptions, Result};
use std::io::{stdout, Write};
use std::sync::Mutex;
use std::os::raw::{c_int, c_char};

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
    _buffered_fd: c_int,       // for st_bstream - we don't use this
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

impl JobuInputStream {
    fn new(content: String) -> Self {
        Self {
            content: content.into_bytes(),
            position: 0,
        }
    }
    
    fn get(&mut self) -> c_int {
        if self.position < self.content.len() {
            let byte = self.content[self.position];
            self.position += 1;
            byte as c_int
        } else {
            -1 // EOF
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
        -1 // EOF if no stream is set
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

// Function to set the input stream from Rust
pub fn set_jobu_input(content: String) {
    let mut stream = JOBU_INPUT.lock().unwrap();
    *stream = Some(JobuInputStream::new(content));
    
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
            StreamType::StString,
            name.as_ptr(),
            location,
        );
        
        // Keep the name alive by leaking it (bash will use it)
        std::mem::forget(name);
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
        // No options: print the current value and increment it.
        if args.is_empty() {
            return Ok(());
        }


        for opt in args.options() {
            match opt? {
                Opt::Set => {
                    // Set the custom input stream for bash
                    set_jobu_input("echo 'Hello from jobu!'\nsleep 1\necho asdf\nexit".to_string());
                    writeln!(stdout(), "Input stream set to jobu")?;
                }
            }
        }

        // It is an error if we receive free arguments.
        args.finished()?;

        Ok(())
    }
}
