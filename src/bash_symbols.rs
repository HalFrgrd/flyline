use libc::{c_char, c_int, c_uint};
use std::fmt::Debug;

pub const EOF: c_int = -1;

pub const BUILTIN_ENABLED: c_int = 0x01;

// common.h seval flags
pub const SEVAL_NOHIST: c_int = 0x004;
pub const SEVAL_NOOPTIMIZE: c_int = 0x400; /* don't try to set optimization flags */

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
    None = 0,
    Stdin = 1,
    Stream = 2,
    String = 3,
    BStream = 4,
}

// INPUT_STREAM union from bash
#[repr(C)]
pub union InputStreamLocation {
    pub string: *mut c_char,
    _file: *mut libc::c_void, // FILE* - we don't use this
    _buffered_fd: c_int,      // for bstream - we don't use this
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
    pub static interactive: c_int;
    pub static interactive_shell: c_int;

    // from shell.h
    pub static no_line_editing: c_int;

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

    // from bashline.c
    // char ** bash_default_completion (const char *text, int start, int end, int qc, int compflags)
    pub fn bash_default_completion(
        text: *const c_char,
        start: c_int,
        end: c_int,
        qc: c_int,
        compflags: c_int,
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

    /* Set to a non-zero value if readline found quoting anywhere in the word to
    be completed; set before any application completion function is called. */
    // extern int rl_completion_found_quote;
    #[link_name = "rl_completion_found_quote"]
    pub static mut rl_completion_found_quote: c_int;

    /* Set to any quote character readline thinks it finds before any application
    completion function is called. */
    // extern int rl_completion_quote_character;
    #[link_name = "rl_completion_quote_character"]
    pub static mut rl_completion_quote_character: c_int;

    /* Non-zero means that the results of the matches are to be quoted using
    double quotes (or an application-specific quoting mechanism) if the
    filename contains any characters in rl_filename_quote_chars.  This is
    ALWAYS non-zero on entry, and can only be changed within a completion
    entry finder function. */
    // complete.c
    #[link_name = "rl_filename_quoting_desired"]
    pub static mut rl_filename_quoting_desired: c_int;

    // Only in more recent versions of readline / bash so not using it.
    /* Non-zero means we should apply filename-type quoting to all completions
    even if we are not otherwise treating the matches as filenames. This is
    ALWAYS zero on entry, and can only be changed within a completion entry
    finder function. */
    // int rl_full_quoting_desired = 0;
    // #[link_name = "rl_full_quoting_desired"]
    // pub static mut rl_full_quoting_desired: c_int;

    /* Non-zero means that the results of the matches are to be treated
    as filenames.  This is ALWAYS zero on entry, and can only be changed
    within a completion entry finder function. */
    // int rl_filename_completion_desired = 0;
    #[link_name = "rl_filename_completion_desired"]
    pub static mut rl_filename_completion_desired: c_int;

    // This isn't set by any completion script.
    // Bash sets it in command_subst_completion_function but I can't figure out how to trigger that.
    /* If non-zero, the completion functions don't append any closing quote.
    This is set to 0 by rl_complete_internal and may be changed by an
    application-specific completion function. */
    // complete.c
    // int rl_completion_suppress_quote = 0;
    // #[link_name = "rl_completion_suppress_quote"]
    // pub static mut rl_completion_suppress_quote: c_int;

    /* If non-zero, the completion functions don't append anything except a
    possible closing quote.  This is set to 0 by rl_complete_internal and
    may be changed by an application-specific completion function. */
    // int rl_completion_suppress_append = 0;
    #[link_name = "rl_completion_suppress_append"]
    pub static mut rl_completion_suppress_append: c_int;

    /* Character appended to completed words when at the end of the line.  The
    default is a space. */
    // int rl_completion_append_character = ' ';
    #[link_name = "rl_completion_append_character"]
    pub static mut rl_completion_append_character: c_int;

    /* If non-zero, sort the completion matches.  On by default. */
    // int rl_sort_completion_matches = 1;
    #[link_name = "rl_sort_completion_matches"]
    pub static mut rl_sort_completion_matches: c_int;

    // typedef char *rl_dequote_func_t (char *, int);
    // rl_dequote_func_t *rl_filename_dequoting_function
    pub static mut rl_filename_dequoting_function:
        Option<extern "C" fn(*const c_char, c_int) -> *mut c_char>;

    // typedef char *rl_quote_func_t (char *, int, char *);
    // rl_quote_func_t *rl_filename_quoting_function
    pub static mut rl_filename_quoting_function:
        Option<extern "C" fn(*const c_char, c_int, *const c_char) -> *mut c_char>;

    // void pcomp_set_readline_variables (int flags, int nval)
    pub fn pcomp_set_readline_variables(flags: c_int, nval: c_int);

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

    // variables.h
    // SHELL_VAR * find_variable (const char *name)
    pub fn find_variable(name: *const c_char) -> *mut ShellVar;

    // common.h
    // int evalstring (char *string, const char *from_file, int flags)
    pub fn evalstring(string: *mut c_char, from_file: *const c_char, flags: c_int) -> c_int;

    // y.tab.c
    // char * decode_prompt_string (char *string, int is_prompt)
    pub fn decode_prompt_string(string: *const c_char, is_prompt: c_int) -> *mut c_char;

    // char *expand_string_to_string (string, quoted)
    pub fn expand_string_to_string(string: *const c_char, quoted: c_int) -> *mut c_char;

    // xmalloc.h
    pub fn xmalloc(size: libc::size_t) -> *mut libc::c_void;
    pub fn xrealloc(ptr: *mut libc::c_void, size: libc::size_t) -> *mut libc::c_void;
    pub fn xfree(ptr: *mut libc::c_void);

    // Signal handling
    /* Set to the value of any terminating signal received. */
    // volatile sig_atomic_t terminating_signal = 0;
    #[link_name = "terminating_signal"]
    pub static mut terminating_signal: c_int;

    // void termsig_handler (int sig)
    pub fn termsig_handler(sig: c_int);

    // rl_hook_func_t *rl_signal_event_hook = (rl_hook_func_t *)NULL;
    pub static mut rl_signal_event_hook: Option<extern "C" fn()>;

    /* If this is non-zero, do job control. */
    // int job_control = 1;
    #[link_name = "job_control"]
    pub static mut job_control: c_int;

    /* Give the terminal to PGRP.  */
    // int give_terminal_to (pid_t pgrp, int force)
    pub fn give_terminal_to(pgrp: libc::pid_t, force: c_int) -> c_int;

    /* The shell's process group. */
    // pid_t shell_pgrp = NO_PID;
    #[link_name = "shell_pgrp"]
    pub static mut shell_pgrp: libc::pid_t;

    /* The value returned by the last synchronous command. */
    #[link_name = "last_command_exit_value"]
    pub static mut last_command_exit_value: c_int;

    // char * get_working_directory (const char *for_whom)
    pub fn get_working_directory(for_whom: *const c_char) -> *mut c_char;

    // char *current_host_name = (char *)NULL;
    #[link_name = "current_host_name"]
    pub static mut current_host_name: *mut c_char;

    // extern int show_var_attributes (SHELL_VAR *, int, int);
    pub fn show_var_attributes(var: *mut ShellVar, flags: c_int, output_fd: c_int) -> c_int;
}

/// Allocate a copy of `s` using bash's `xmalloc`.
///
/// # Safety
/// Must be called while bash is loaded. The returned pointer must be freed with
/// `xfree` or passed to bash (which will free it with `xfree`).
pub unsafe fn xmalloc_cstr(s: &std::ffi::CStr) -> *mut c_char {
    let bytes = s.to_bytes_with_nul();
    unsafe {
        let ptr = xmalloc(bytes.len()) as *mut c_char;
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, ptr, bytes.len());
        ptr
    }
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
#[derive(Clone)]
pub struct ShellVar {
    pub name: *mut c_char,      // Symbol that the user types.
    pub value: *mut c_char,     // Value that is returned.
    pub exportstr: *mut c_char, // String for the environment.
    pub dynamic_value: Option<extern "C" fn() -> *mut c_char>, // Function called to return a `dynamic' value for a variable, like $SECONDS or $RANDOM.
    pub assign_func: Option<extern "C" fn(*const c_char)>, // Function called when this `special variable' is assigned a value in bind_variable.
    pub attributes: c_int,                                 // export, readonly, array, invisible...
    pub context: c_int, // Which context this variable belongs to.
}

impl ShellVar {
    pub fn get_value(&self) -> Option<String> {
        if self.value.is_null() {
            None
        } else {
            unsafe {
                Some(
                    std::ffi::CStr::from_ptr(self.value)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_name(&self) -> Option<String> {
        if self.name.is_null() {
            None
        } else {
            unsafe {
                Some(
                    std::ffi::CStr::from_ptr(self.name)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_exportstr(&self) -> Option<String> {
        if self.exportstr.is_null() {
            None
        } else {
            unsafe {
                Some(
                    std::ffi::CStr::from_ptr(self.exportstr)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn is_exported(&self) -> bool {
        self.attributes & ATT_EXPORTED as c_int != 0
    }

    pub fn is_readonly(&self) -> bool {
        self.attributes & ATT_READONLY as c_int != 0
    }

    pub fn is_array(&self) -> bool {
        self.attributes & ATT_ARRAY as c_int != 0
    }

    pub fn is_function(&self) -> bool {
        self.attributes & ATT_FUNCTION as c_int != 0
    }

    pub fn is_integer(&self) -> bool {
        self.attributes & ATT_INTEGER as c_int != 0
    }

    pub fn is_local(&self) -> bool {
        self.attributes & ATT_LOCAL as c_int != 0
    }

    pub fn is_associative_array(&self) -> bool {
        self.attributes & ATT_ASSOC as c_int != 0
    }
}

impl Debug for ShellVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellVar")
            .field("name", &self.get_name())
            .field("value", &self.get_value())
            .field("exportstr", &self.get_exportstr())
            .field("attributes", &self.attributes)
            .field("is_exported", &self.is_exported())
            .field("is_readonly", &self.is_readonly())
            .field("is_array", &self.is_array())
            .field("is_function", &self.is_function())
            .field("is_integer", &self.is_integer())
            .field("is_local", &self.is_local())
            .field("is_associative_array", &self.is_associative_array())
            .finish()
    }
}

/* The various attributes that a given variable can have. */
/* First, the user-visible attributes */
#[allow(unused)]
pub const ATT_EXPORTED: libc::c_ulong = 0x0000001; /* export to environment */
#[allow(unused)]
pub const ATT_READONLY: libc::c_ulong = 0x0000002; /* cannot change */
#[allow(unused)]
pub const ATT_ARRAY: libc::c_ulong = 0x0000004; /* value is an array */
#[allow(unused)]
pub const ATT_FUNCTION: libc::c_ulong = 0x0000008; /* value is a function */
#[allow(unused)]
pub const ATT_INTEGER: libc::c_ulong = 0x0000010; /* internal representation is int */
#[allow(unused)]
pub const ATT_LOCAL: libc::c_ulong = 0x0000020; /* variable is local to a function */
#[allow(unused)]
pub const ATT_ASSOC: libc::c_ulong = 0x0000040; /* variable is an associative array */
#[allow(unused)]
pub const ATT_TRACE: libc::c_ulong = 0x0000080; /* function is traced with DEBUG trap */
#[allow(unused)]
pub const ATT_UPPERCASE: libc::c_ulong = 0x0000100; /* word converted to uppercase on assignment */
#[allow(unused)]
pub const ATT_LOWERCASE: libc::c_ulong = 0x0000200; /* word converted to lowercase on assignment */
#[allow(unused)]
pub const ATT_CAPCASE: libc::c_ulong = 0x0000400; /* word capitalized on assignment */
#[allow(unused)]
pub const ATT_NAMEREF: libc::c_ulong = 0x0000800; /* word is a name reference */

/* Internal attributes used for bookkeeping */
#[allow(unused)]
pub const ATT_INVISIBLE: libc::c_ulong = 0x0001000; /* cannot see */
#[allow(unused)]
pub const ATT_NOUNSET: libc::c_ulong = 0x0002000; /* cannot unset */
#[allow(unused)]
pub const ATT_NOASSIGN: libc::c_ulong = 0x0004000; /* assignment not allowed */
#[allow(unused)]
pub const ATT_IMPORTED: libc::c_ulong = 0x0008000; /* came from environment */
#[allow(unused)]
pub const ATT_SPECIAL: libc::c_ulong = 0x0010000; /* requires special handling */
#[allow(unused)]
pub const ATT_NOFREE: libc::c_ulong = 0x0020000; /* do not free value on unset */
#[allow(unused)]
pub const ATT_REGENERATE: libc::c_ulong = 0x0040000; /* regenerate when exported */

/* Internal attributes used for variable scoping. */
#[allow(unused)]
pub const ATT_TEMPVAR: libc::c_ulong = 0x0100000; /* variable came from the temp environment */
#[allow(unused)]
pub const ATT_PROPAGATE: libc::c_ulong = 0x0200000; /* propagate to previous scope */

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

pub fn set_readline_state(state: libc::c_ulong) {
    unsafe {
        rl_readline_state |= state;
    }
}

pub fn clear_readline_state(state: libc::c_ulong) {
    unsafe {
        rl_readline_state &= !state;
    }
}

#[allow(unused)]
pub const RL_STATE_NONE: libc::c_ulong = 0x0000000; /* no state; before first call */
#[allow(unused)]
pub const RL_STATE_INITIALIZING: libc::c_ulong = 0x00000001; /* initializing */
#[allow(unused)]
pub const RL_STATE_INITIALIZED: libc::c_ulong = 0x00000002; /* initialization done */
#[allow(unused)]
pub const RL_STATE_TERMPREPPED: libc::c_ulong = 0x00000004; /* terminal is prepped */
#[allow(unused)]
pub const RL_STATE_READCMD: libc::c_ulong = 0x00000008; /* reading a command key */
#[allow(unused)]
pub const RL_STATE_METANEXT: libc::c_ulong = 0x00000010; /* reading input after ESC */
#[allow(unused)]
pub const RL_STATE_DISPATCHING: libc::c_ulong = 0x00000020; /* dispatching to a command */
#[allow(unused)]
pub const RL_STATE_MOREINPUT: libc::c_ulong = 0x00000040; /* reading more input in a command function */
#[allow(unused)]
pub const RL_STATE_ISEARCH: libc::c_ulong = 0x00000080; /* doing incremental search */
#[allow(unused)]
pub const RL_STATE_NSEARCH: libc::c_ulong = 0x00000100; /* doing non-inc search */
#[allow(unused)]
pub const RL_STATE_SEARCH: libc::c_ulong = 0x00000200; /* doing a history search */
#[allow(unused)]
pub const RL_STATE_NUMERICARG: libc::c_ulong = 0x00000400; /* reading numeric argument */
#[allow(unused)]
pub const RL_STATE_MACROINPUT: libc::c_ulong = 0x00000800; /* getting input from a macro */
#[allow(unused)]
pub const RL_STATE_MACRODEF: libc::c_ulong = 0x00001000; /* defining keyboard macro */
#[allow(unused)]
pub const RL_STATE_OVERWRITE: libc::c_ulong = 0x00002000; /* overwrite mode */
#[allow(unused)]
pub const RL_STATE_COMPLETING: libc::c_ulong = 0x00004000; /* doing completion */
#[allow(unused)]
pub const RL_STATE_SIGHANDLER: libc::c_ulong = 0x00008000; /* in readline sighandler */
#[allow(unused)]
pub const RL_STATE_UNDOING: libc::c_ulong = 0x00010000; /* doing an undo */
#[allow(unused)]
pub const RL_STATE_INPUTPENDING: libc::c_ulong = 0x00020000; /* rl_execute_next called */
#[allow(unused)]
pub const RL_STATE_TTYCSAVED: libc::c_ulong = 0x00040000; /* tty special chars saved */
#[allow(unused)]
pub const RL_STATE_CALLBACK: libc::c_ulong = 0x00080000; /* using the callback interface */
#[allow(unused)]
pub const RL_STATE_VIMOTION: libc::c_ulong = 0x00100000; /* reading vi motion arg */
#[allow(unused)]
pub const RL_STATE_MULTIKEY: libc::c_ulong = 0x00200000; /* reading multiple-key command */
#[allow(unused)]
pub const RL_STATE_VICMDONCE: libc::c_ulong = 0x00400000; /* entered vi command mode at least once */
#[allow(unused)]
pub const RL_STATE_CHARSEARCH: libc::c_ulong = 0x00800000; /* vi mode char search */
#[allow(unused)]
pub const RL_STATE_REDISPLAYING: libc::c_ulong = 0x01000000; /* updating terminal display */
#[allow(unused)]
pub const RL_STATE_DONE: libc::c_ulong = 0x02000000; /* done; accepted line */
#[allow(unused)]
pub const RL_STATE_TIMEOUT: libc::c_ulong = 0x04000000; /* done; timed out */
#[allow(unused)]
pub const RL_STATE_EOF: libc::c_ulong = 0x08000000; /* done; got eof on read */
#[allow(unused)]
pub const RL_STATE_READSTR: libc::c_ulong = 0x10000000; /* reading a string for M-x */
