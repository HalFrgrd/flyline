use libc::{c_char, c_int, c_uint};

pub const EOF: c_int = -1;

pub const BUILTIN_ENABLED: c_int = 0x01;
/* A structure which represents a word. */
// typedef struct word_desc {
//   char *word;		/* Zero terminated string. */
//   int flags;		/* Flags associated with this word. */
// } WORD_DESC;
#[repr(C)]
#[allow(dead_code)]
pub struct WordDesc {
    pub word: *const c_char, // Zero terminated string.
    pub flags: c_int,        // Flags associated with this word.
}

/* A linked list of words. */
// typedef struct word_list {
//   struct word_list *next;
//   WORD_DESC *word;
// } WORD_LIST;
#[repr(C)]
#[allow(dead_code)]
pub struct WordList {
    pub next: *const WordList,
    pub word: *const WordDesc,
}

pub type BashBuiltinCallFunc = extern "C" fn(*const WordList) -> c_int;

/* The thing that we build the array of builtins out of. */
// struct builtin {
//   char *name;			/* The name that the user types. */
//   sh_builtin_func_t *function;	/* The address of the invoked function. */
//   int flags;			/* One of the #defines above. */
//   char * const *long_doc;	/* NULL terminated array of strings. */
//   const char *short_doc;	/* Short version of documentation. */
//   char *handle;			/* for future use */
// };
#[repr(C)]
#[allow(dead_code)]
pub struct BashBuiltin {
    pub name: *const c_char,                   // The name that the user types.
    pub function: Option<BashBuiltinCallFunc>, // The address of the invoked function.
    pub flags: c_int,                          // One of the #defines above.
    pub long_doc: *const *const c_char,        // NULL terminated array of strings.
    pub short_doc: *const c_char,              // Short version of documentation.
    pub handle: *const c_char,                 // for future use
}

// shell.h
#[repr(i32)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinExitCode {
    ExecutionSuccess = 0,
    BadSyntax = 257,    // shell syntax error
    Usage = 258,        // syntax error in usage
    RedirFail = 259,    // redirection failed
    BadAssign = 260,    // variable assignment error
    ExpFail = 261,      // word expansion failed
    DiskFallback = 262, // fall back to disk command from builtin
    UtilError = 263,    // Posix special builtin utility error
}

// Bash input stream types from bash's input.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub type ShCGetFunc = unsafe extern "C" fn() -> c_int;
#[allow(dead_code)]
pub type ShCUngetFunc = unsafe extern "C" fn(c_int) -> c_int;

// BASH_INPUT structure from bash's input.h
#[repr(C)]
#[allow(dead_code)]
pub struct BashInput {
    pub stream_type: StreamType,
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
#[allow(dead_code)]
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

#[allow(dead_code)]
unsafe extern "C" {

    // stream_list global from y.tab.c
    #[link_name = "stream_list"]
    pub static mut stream_list: *mut StreamSaver;

    // input.h
    // extern BASH_INPUT bash_input;
    #[link_name = "bash_input"]
    pub static mut bash_input: BashInput;

    // input.h
    // void push_stream (int reset_lineno)
    pub fn push_stream(reset_lineno: c_int);

    // input.h
    // void pop_stream (void)
    pub fn pop_stream();

    // from shell.h
    pub static interactive_shell: c_int;

    // y.tab.c
    // void with_input_from_stdin (void)
    pub fn with_input_from_stdin();

    // alias.h
    /* Return the value of the alias for NAME, or NULL if there is none. */
    // extern char *get_alias_value (const char *);
    pub fn get_alias_value(name: *const c_char) -> *mut c_char;

    // from type.def
    // int describe_command (char *command, int dflags)
    pub fn describe_command(command: *const c_char, dflags: c_int) -> c_int;

    // from pcomplete.c
    /* The driver function for the programmable completion code.  Returns a list
    of matches for WORD, which is an argument to command CMD.  START and END
    bound the command currently being completed in pcomp_line (usually
    rl_line_buffer). */
    // char ** programmable_completions (const char *cmd, const char *word, int start, int end, int *foundp)
    pub fn programmable_completions(
        cmd: *const c_char,
        word: *const c_char,
        start: c_int,
        end: c_int,
        foundp: *mut c_int,
    ) -> *mut *mut c_char;

    // from readline/readline.h
    // Line buffer and maintenance
    // char *rl_line_buffer
    #[link_name = "rl_line_buffer"]
    pub static mut rl_line_buffer: *mut c_char;

    /* The location of point, and end. */
    // extern int rl_point;
    #[link_name = "rl_point"]
    pub static mut rl_point: c_int;

    // extern int rl_end;
    #[link_name = "rl_end"]
    pub static mut rl_end: c_int;

    // alias.h
    // alias_t **all_aliases (void);
    pub fn all_aliases() -> *mut *mut Alias;

    // char **all_variables_matching_prefix (const char *prefix)
    pub fn all_variables_matching_prefix(prefix: *const c_char) -> *mut *mut c_char;

    // extern SHELL_VAR **all_shell_functions (void);
    pub fn all_shell_functions() -> *mut *mut ShellVar;

    // extern struct builtin *shell_builtins;
    #[link_name = "shell_builtins"]
    pub static mut shell_builtins: *mut BashBuiltinType;

    // num_shell_builtins
    #[link_name = "num_shell_builtins"]
    pub static mut num_shell_builtins: c_int;

    //extern unsigned long rl_readline_state;
    #[link_name = "rl_readline_state"]
    pub static mut rl_readline_state: libc::c_ulong;

    // int current_command_line_count;
    #[link_name = "current_command_line_count"]
    pub static mut current_command_line_count: c_int;

    // extern HIST_ENTRY **history_list (void);
    pub fn history_list() -> *mut *mut HistoryEntry;

    // y.tab.c
    // char *current_readline_prompt
    #[link_name = "current_readline_prompt"]
    pub static mut current_readline_prompt: *mut c_char;

    // getenv.c
    // char* getenv(const char* name);
    pub fn getenv(name: *const c_char) -> *mut c_char;

    // y.tab.c
    // char * decode_prompt_string (char *string, int is_prompt)
    pub fn decode_prompt_string(string: *const c_char, is_prompt: c_int) -> *mut c_char;
}

// history.h
pub type HistdataT = *mut libc::c_void;

// history.h
#[repr(C)]
#[allow(dead_code)]
pub struct HistoryEntry {
    pub line: *mut c_char,
    pub timestamp: *mut c_char,
    pub data: HistdataT,
}

// pcomplete.h
#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct CompSpec {
    pub refcount: c_int,
    pub actions: libc::c_ulong,
    pub options: libc::c_ulong,
    pub globpat: *mut c_char,
    pub words: *mut c_char,
    pub prefix: *mut c_char,
    pub suffix: *mut c_char,
    pub funcname: *mut c_char,
    pub command: *mut c_char,
    pub lcommand: *mut c_char,
    pub filterpat: *mut c_char,
}

// alias.h
#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct Alias {
    pub name: *mut c_char,
    pub value: *mut c_char,
    pub flags: c_char,
}

// variables.h
#[repr(C)]
#[allow(dead_code)]
pub struct ShellVar {
    pub name: *mut c_char,      // Symbol that the user types.
    pub value: *mut c_char,     // Value that is returned.
    pub exportstr: *mut c_char, // String for the environment.
    pub dynamic_value: Option<extern "C" fn() -> *mut c_char>, // Function called to return a `dynamic' value for a variable, like $SECONDS or $RANDOM.
    pub assign_func: Option<extern "C" fn(*const c_char)>, // Function called when this `special variable' is assigned a value in bind_variable.
    pub attributes: c_int,                                 // export, readonly, array, invisible...
    pub context: c_int, // Which context this variable belongs to.
}

// builtins.h
// };
#[repr(C)]
#[allow(dead_code)]
pub struct BashBuiltinType {
    pub name: *mut c_char, // The name that the user types.
    pub function: Option<extern "C" fn(c_int, *mut *mut c_char, *mut c_char) -> c_int>, // The address of the invoked function.
    pub flags: c_int,               // One of the #defines above.
    pub long_doc: *mut *mut c_char, // NULL terminated array of strings.
    pub short_doc: *mut c_char,     // Short version of documentation.
    pub handle: *mut c_char,        // for future use
}

// externs.h
#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct StringList {
    pub list: *mut *mut c_char,
    pub list_size: c_uint, // TODO verify this is the correct type
    pub list_len: c_uint,
}
