use std::os::raw::{c_char, c_int};

use libc::c_uint;

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

    // stream_list global from y.tab.c
    #[link_name = "stream_list"]
    pub static mut stream_list: *mut StreamSaver;


    // from shell.h
    pub static interactive_shell: c_int;

    // alias.h
    /* Return the value of the alias for NAME, or NULL if there is none. */
    // extern char *get_alias_value (const char *);
    pub fn get_alias_value(name: *const c_char) -> *mut c_char;

    // from type.def
    // int describe_command (char *command, int dflags)
    pub fn describe_command(command: *const c_char, dflags: c_int) -> c_int;

    // from pcomplete.c
    // COMPSPEC *progcomp_search (const char *cmd)
    pub fn progcomp_search(cmd: *const c_char) -> *mut CompSpec;

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

    // int rl_line_buffer_len
    #[link_name = "rl_line_buffer_len"]
    pub static mut rl_line_buffer_len: c_int;

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

    // STRINGLIST * gen_compspec_completions (COMPSPEC *cs, const char *cmd, const char *word,int start, int end, int *foundp)
    pub fn gen_compspec_completions(
        cs: *mut CompSpec,
        cmd: *const c_char,
        word: *const c_char,
        start: c_int,
        end: c_int,
        foundp: *mut c_int,
    ) -> *mut StringList;

    // COMPSPEC *pcomp_curcs;
    pub static mut pcomp_curcs: *mut CompSpec;

    // char *pcomp_line;
    #[link_name = "pcomp_line"]
    pub static mut pcomp_line: *mut c_char;

    // int pcomp_ind;
    #[link_name = "pcomp_ind"]
    pub static mut pcomp_ind: c_int;

    //extern unsigned long rl_readline_state;
    #[link_name = "rl_readline_state"]
    pub static mut rl_readline_state: libc::c_ulong;

    // int current_command_line_count;
    #[link_name = "current_command_line_count"]
    pub static mut current_command_line_count: c_int;

}

// COMPSPEC structure from pcomplete.h
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

// typedef struct alias {
//   char *name;
//   char *value;
//   char flags;
// } alias_t;
#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct Alias {
    pub name: *mut c_char,
    pub value: *mut c_char,
    pub flags: c_char,
}

// typedef struct variable {
//   char *name;			/* Symbol that the user types. */
//   char *value;			/* Value that is returned. */
//   char *exportstr;		/* String for the environment. */
//   sh_var_value_func_t *dynamic_value;	/* Function called to return a `dynamic'
// 				   value for a variable, like $SECONDS
// 				   or $RANDOM. */
//   sh_var_assign_func_t *assign_func; /* Function called when this `special
// 				   variable' is assigned a value in
// 				   bind_variable. */
//   int attributes;		/* export, readonly, array, invisible... */
//   int context;			/* Which context this variable belongs to. */
// } SHELL_VAR;
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
pub struct BashBuiltinType {
    pub name: *mut c_char, // The name that the user types.
    pub function: Option<extern "C" fn(c_int, *mut *mut c_char, *mut c_char) -> c_int>, // The address of the invoked function.
    pub flags: c_int,               // One of the #defines above.
    pub long_doc: *mut *mut c_char, // NULL terminated array of strings.
    pub short_doc: *mut c_char,     // Short version of documentation.
    pub handle: *mut c_char,        // for future use
}

// typedef struct _list_of_strings {
//   char **list;
//   size_t list_size;
//   size_t list_len;
// } STRINGLIST;
#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct StringList {
    pub list: *mut *mut c_char,
    pub list_size: c_uint, // TODO verify this is the correct type
    pub list_len: c_uint,
}

// typedef struct compspec {
//   int refcount;
//   unsigned long actions;
//   unsigned long options;
//   char *globpat;
//   char *words;
//   char *prefix;
//   char *suffix;
//   char *funcname;
//   char *command;
//   char *lcommand;
//   char *filterpat;
// } COMPSPEC;

// COMPSPEC *
// progcomp_search (const char *cmd)
// {
//   register BUCKET_CONTENTS *item;
//   COMPSPEC *cs;

//   if (prog_completes == 0)
//     return ((COMPSPEC *)NULL);

//   item = hash_search (cmd, prog_completes, 0);

//   if (item == NULL)
//     return ((COMPSPEC *)NULL);

//   cs = (COMPSPEC *)item->data;

//   return (cs);
// }
