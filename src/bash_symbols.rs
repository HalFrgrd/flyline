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

// builtins/common.h
// /* Flags for describe_command, shared between type.def and command.def */
// #define CDESC_ALL		0x001	/* type -a */
// #define CDESC_SHORTDESC		0x002	/* command -V */
// #define CDESC_REUSABLE		0x004	/* command -v */
// #define CDESC_TYPE		0x008	/* type -t */
// #define CDESC_PATH_ONLY		0x010	/* type -p */
// #define CDESC_FORCE_PATH	0x020	/* type -ap or type -P */
// #define CDESC_NOFUNCS		0x040	/* type -f */
// #define CDESC_ABSPATH		0x080	/* convert to absolute path, no ./ */
// #define CDESC_STDPATH		0x100	/* command -p */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CDescFlag {
    All = 0x001,       // CDESC_ALL - type -a
    ShortDesc = 0x002, // CDESC_SHORTDESC - command -V
    Reusable = 0x004,  // CDESC_REUSABLE - command -v
    Type = 0x008,      // CDESC_TYPE - type -t
    PathOnly = 0x010,  // CDESC_PATH_ONLY - type -p
    ForcePath = 0x020, // CDESC_FORCE_PATH - type -ap or type -P
    NoFuncs = 0x040,   // CDESC_NOFUNCS - type -f
    AbsPath = 0x080,   // CDESC_ABSPATH - convert to absolute path, no ./
    StdPath = 0x100,   // CDESC_STDPATH - command -p
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

    // pub fn with_input_from_stdin();

    // stream_list global from y.tab.c
    #[link_name = "stream_list"]
    pub static mut stream_list: *mut StreamSaver;

    // from shell.h
    pub static interactive_shell: c_int;

    // from type.def
    // int describe_command (char *command, int dflags)
    pub fn describe_command(command: *const c_char, dflags: c_int) -> c_int;

}
