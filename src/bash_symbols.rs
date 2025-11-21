use std::os::raw::{c_char, c_int};

pub const EOF: c_int = -1;

// Bash input stream types from bash's input.h
#[repr(C)]
#[allow(dead_code)]
pub enum StreamType {
    StNone = 0,
    StStdin = 1,
    StStream = 2,
    StString = 3,
    StBStream = 4,
}

// INPUT_STREAM union from bash
#[repr(C)]
pub union InputStreamLocation {
    pub string: *mut c_char,
    _file: *mut libc::c_void, // FILE* - we don't use this
    _buffered_fd: c_int,      // for st_bstream - we don't use this
}

// BUFFERED_STREAM from bash's input.h (opaque pointer for our purposes)
#[repr(C)]
#[allow(dead_code)]
pub struct BufferedStream {
    _private: [u8; 0],
}

// sh_cget_func_t and sh_cunget_func_t are function pointer types
#[allow(dead_code)]
pub type ShCGetFunc = extern "C" fn() -> c_int;
#[allow(dead_code)]
pub type ShCUngetFunc = extern "C" fn(c_int) -> c_int;

// BASH_INPUT structure from bash's input.h
#[repr(C)]
#[allow(dead_code)]
pub struct BashInput {
    pub type_: StreamType,
    pub name: *mut c_char,
    pub location: InputStreamLocation,
    pub getter: Option<ShCGetFunc>,
    pub ungetter: Option<ShCUngetFunc>,
}

// STREAM_SAVER structure from bash's y.tab.c
#[repr(C)]
#[allow(dead_code)]
pub struct StreamSaver {
    pub next: *mut StreamSaver,
    pub bash_input: BashInput,
    pub line: c_int,
    pub bstream: *mut BufferedStream,
}

// External bash_input symbol that bash provides
#[allow(dead_code)]
unsafe extern "C" {
    pub fn init_yy_io(
        get: extern "C" fn() -> c_int,
        unget: extern "C" fn(c_int) -> c_int,
        type_: StreamType,
        name: *const c_char,
        location: InputStreamLocation,
    );

    pub fn with_input_from_stdin();

    // stream_list global from y.tab.c
    #[link_name = "stream_list"]
    pub static mut stream_list: *mut StreamSaver;
}
