use crate::bash_symbols;
use crate::bash_symbols::ShellVar;

use anyhow::Result;

use libc::{c_char, c_int};
use lscolors::LsColors;
use ratatui::style::{Color, Modifier, Style};
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

fn with_redirected_stdout<F, R>(func: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    // Create a pipe to capture stdout
    let (read_fd, write_fd) = unsafe {
        let mut fds: [c_int; 2] = [0; 2];
        libc::pipe(fds.as_mut_ptr());
        (fds[0], fds[1])
    };

    // Save original stdout
    let original_stdout = unsafe { libc::dup(libc::STDOUT_FILENO) };

    // Redirect stdout to write end of pipe
    unsafe {
        libc::dup2(write_fd, libc::STDOUT_FILENO);
        libc::close(write_fd);
    };

    // Call the provided function
    let result = func();

    // Flush stdout to ensure all data is written to pipe
    unsafe { libc::fflush(std::ptr::null_mut()) };

    // Restore original stdout
    unsafe {
        libc::dup2(original_stdout, libc::STDOUT_FILENO);
        libc::close(original_stdout);
    };

    // Read from pipe
    let mut output = String::new();
    unsafe {
        let mut read_file = std::fs::File::from_raw_fd(read_fd);
        read_file.read_to_string(&mut output).unwrap();
    };

    (result, output.to_string())
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum CommandType {
    Unknown,
    Alias,
    Keyword,
    Function,
    Builtin,
    File,
}

impl CommandType {
    pub fn from_str(s: &str) -> CommandType {
        match s {
            "alias" => CommandType::Alias,
            "keyword" => CommandType::Keyword,
            "function" => CommandType::Function,
            "builtin" => CommandType::Builtin,
            "file" => CommandType::File,
            _ => CommandType::Unknown,
        }
    }
}

pub fn find_alias(cmd: &str) -> Option<String> {
    unsafe {
        let alias_ptr =
            bash_symbols::get_alias_value(std::ffi::CString::new(cmd).unwrap().as_ptr());
        if alias_ptr.is_null() {
            return None;
        }

        let c_str = std::ffi::CStr::from_ptr(alias_ptr);
        if let Ok(str_slice) = c_str.to_str() {
            return Some(str_slice.to_string());
        }
    }
    None
}

fn get_command_type_uncached(cmd: &str) -> (CommandType, String) {
    // If the command word looks like a filename (contains '/' or starts with
    // '~'), expand it first so that tilde and variable expansion are resolved
    // before the lookup.
    let expanded;
    let cmd = if cmd.starts_with('~') || cmd.contains('/') {
        expanded = fully_expand_path(cmd);
        if expanded.is_empty() { cmd } else { &expanded }
    } else {
        cmd
    };

    // Call the `type` builtin to check if the command exists
    let cmd_c_str = std::ffi::CString::new(cmd).unwrap();

    let (_, command_type_output) = with_redirected_stdout(|| unsafe {
        bash_symbols::describe_command(cmd_c_str.as_ptr(), bash_symbols::CDescFlag::Type as c_int)
    });
    let command_type = CommandType::from_str(command_type_output.trim());

    let (_, short_desc) = match command_type {
        CommandType::Alias => {
            let (result, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::ShortDesc as c_int,
                )
            });
            let extracted = if let Some(start) = output.find('`') {
                if let Some(end) = output.rfind('\'') {
                    output[start + 1..end].to_string()
                } else {
                    output
                }
            } else {
                output
            };
            (result, format!("alias: {}", extracted))
        }
        CommandType::Builtin | CommandType::Keyword => {
            let (result, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::ShortDesc as c_int,
                )
            });

            (
                result,
                format!("{}: {}", command_type_output.trim(), output.trim()),
            )
        }
        CommandType::File => {
            let (result, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::PathOnly as c_int,
                )
            });

            (result, format!("file: {}", output.trim()))
        }
        CommandType::Function => {
            (0, "function".to_string()) // For functions, we currently don't extract a short description
        }
        CommandType::Unknown => {
            // If unknown, no short description
            (0, "unknown".to_string())
        }
    };

    (command_type, short_desc)
}

static CALL_TYPE_CACHE: Mutex<Option<HashMap<String, (CommandType, String)>>> = Mutex::new(None);

pub fn get_command_info(cmd: &str) -> (CommandType, String) {
    let mut cache_guard = CALL_TYPE_CACHE.lock().unwrap();
    let cache = cache_guard.get_or_insert_with(HashMap::new);

    if let Some(res) = cache.get(cmd) {
        res.clone()
    } else {
        let result = get_command_type_uncached(cmd);
        cache.insert(cmd.to_string(), result.clone());
        result
    }
}

pub fn format_shell_var_uncached(name: &str) -> String {
    get_shell_var(name)
        .and_then(|mut var| {
            let (res, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::show_var_attributes(&mut var, 0, 0)
            });
            if res != 0 {
                None
            } else {
                Some(output.trim().to_string())
            }
        })
        .map(|output| {
            if let Some(pos) = output.find(name) {
                format!("${}", output[pos..].trim())
            } else {
                output.trim().to_string()
            }
        })
        .unwrap_or_else(|| format!("${}=", name))
}

static SHELL_VAR_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

pub fn format_shell_var(name: &str) -> String {
    let mut cache_guard = SHELL_VAR_CACHE.lock().unwrap();
    let cache = cache_guard.get_or_insert_with(HashMap::new);

    if let Some(res) = cache.get(name) {
        res.clone()
    } else {
        let result = format_shell_var_uncached(name);
        cache.insert(name.to_string(), result.clone());
        result
    }
}

pub fn reset_caches() {
    let mut cache_guard = CALL_TYPE_CACHE.lock().unwrap();
    *cache_guard = None;

    let mut cache_guard = SHELL_VAR_CACHE.lock().unwrap();
    *cache_guard = None;
}

pub fn get_all_aliases() -> Vec<String> {
    // TODO can we extract more info here?
    let mut aliases = Vec::new();

    unsafe {
        let alias_ptr = bash_symbols::all_aliases();
        if alias_ptr.is_null() {
            return aliases;
        }

        let mut offset = 0;
        loop {
            let ptr = *alias_ptr.add(offset);
            if ptr.is_null() {
                break;
            }
            let alias = &*ptr;
            if !alias.name.is_null() {
                let c_str = std::ffi::CStr::from_ptr(alias.name);
                if let Ok(str_slice) = c_str.to_str() {
                    aliases.push(str_slice.to_string());
                }
            }
            offset += 1;
        }
    }

    aliases
}

pub fn get_all_reserved_words() -> Vec<String> {
    vec![
        "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until",
        "do", "done", "in", "function", "time", "{", "}", "!", "[[", "]]", "coproc",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn get_all_variables_with_prefix(prefix: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let prefix_c_str = std::ffi::CString::new(prefix.strip_prefix('$').unwrap_or(prefix)).unwrap();

    unsafe {
        let var_ptr = bash_symbols::all_variables_matching_prefix(prefix_c_str.as_ptr());
        if var_ptr.is_null() {
            return variables;
        }

        let mut offset = 0;
        loop {
            let ptr = *var_ptr.add(offset);
            if ptr.is_null() {
                break;
            }
            let c_str = std::ffi::CStr::from_ptr(ptr);
            if let Ok(str_slice) = c_str.to_str() {
                variables.push(format!("${}", str_slice));
            }
            offset += 1;
        }
    }

    log::debug!("Found variables with prefix '{}': {:?}", prefix, variables);
    variables
}

pub fn get_all_shell_functions() -> Vec<String> {
    let mut functions = Vec::new();

    unsafe {
        let func_ptr = bash_symbols::all_shell_functions();
        if func_ptr.is_null() {
            return functions;
        }

        let mut offset = 0;
        loop {
            let ptr = *func_ptr.add(offset);
            if ptr.is_null() {
                break;
            }
            let shell_var = &*ptr;
            if !shell_var.name.is_null() {
                let c_str = std::ffi::CStr::from_ptr(shell_var.name);
                if let Ok(str_slice) = c_str.to_str() {
                    functions.push(str_slice.to_string());
                }
            }
            offset += 1;
        }
    }

    // log::debug!("Found shell functions: {:?}", functions);
    functions
}

pub fn get_all_shell_builtins() -> Vec<String> {
    let mut builtins = Vec::new();

    unsafe {
        let builtin_ptr = bash_symbols::shell_builtins;
        if builtin_ptr.is_null() {
            return builtins;
        }

        let num_builtins = bash_symbols::num_shell_builtins as isize;
        for i in 0..num_builtins {
            let bash_builtin = &*builtin_ptr.offset(i);
            if !bash_builtin.name.is_null() {
                let c_str = std::ffi::CStr::from_ptr(bash_builtin.name);
                if let Ok(str_slice) = c_str.to_str() {
                    builtins.push(str_slice.to_string());
                }
            }
        }
    }

    // log::debug!("Found shell builtins: {:?}", builtins);
    builtins
}

/* Values for COMPSPEC options field. */
// #define COPT_RESERVED	(1<<0)		/* reserved for other use */
// #define COPT_DEFAULT	(1<<1)
// #define COPT_FILENAMES	(1<<2)
// #define COPT_DIRNAMES	(1<<3)
// #define COPT_NOQUOTE	(1<<4)
// #define COPT_NOSPACE	(1<<5)
// #define COPT_BASHDEFAULT (1<<6)
// #define COPT_PLUSDIRS	(1<<7)
// #define COPT_NOSORT	(1<<8)
// #define COPT_FULLQUOTE	(1<<9)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompspecOption {
    Reserved = 1 << 0,
    Default = 1 << 1,
    Filenames = 1 << 2,
    Dirnames = 1 << 3,
    NoQuote = 1 << 4,
    NoSpace = 1 << 5,
    BashDefault = 1 << 6,
    PlusDirs = 1 << 7,
    NoSort = 1 << 8,
    FullQuote = 1 << 9,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CompletionFlags {
    pub quote_type: Option<QuoteType>,

    pub readline_default_fallback_desired: bool,
    // pub dirnames_desired: bool, // Bash handles this already during call to programmable_completions
    // pub plus_dirs: bool, // Likewise
    pub filename_quoting_desired: bool,
    pub filename_completion_desired: bool,
    pub no_suffix_desired: bool,
    pub suffix_character: char,
    pub bash_default_fallback_desired: bool,
    pub nosort_desired: bool,
    // pub full_quote: bool,
}

impl CompletionFlags {
    pub fn from(quote_type: Option<QuoteType>, foundcs: c_int, append_char: i32) -> Self {
        Self {
            quote_type,
            readline_default_fallback_desired: foundcs & (CompspecOption::Default as c_int) != 0,
            filename_quoting_desired: foundcs & (CompspecOption::NoQuote as c_int) == 0,
            filename_completion_desired: foundcs & (CompspecOption::Filenames as c_int) != 0,
            no_suffix_desired: foundcs & (CompspecOption::NoSpace as c_int) != 0,
            suffix_character: char::from_u32(append_char as u32).unwrap_or(' '),
            bash_default_fallback_desired: foundcs & (CompspecOption::BashDefault as c_int) != 0,
            nosort_desired: foundcs & (CompspecOption::NoSort as c_int) != 0,
        }
    }
}

impl Default for CompletionFlags {
    fn default() -> Self {
        Self {
            quote_type: None,
            readline_default_fallback_desired: true,
            filename_quoting_desired: true,
            filename_completion_desired: false,
            no_suffix_desired: false,
            suffix_character: ' ',
            bash_default_fallback_desired: false,
            nosort_desired: false,
        }
    }
}

pub struct ProgrammableCompleteReturn {
    pub completions: Vec<String>,
    pub flags: CompletionFlags,
}

impl std::fmt::Debug for ProgrammableCompleteReturn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MAX_DISPLAY: usize = 50;
        let mut s = f.debug_struct("ProgrammableCompleteReturn");
        if self.completions.len() <= MAX_DISPLAY {
            s.field("completions", &self.completions);
        } else {
            s.field(
                "completions",
                &format_args!(
                    "({} total, showing first {}) {:?}",
                    self.completions.len(),
                    MAX_DISPLAY,
                    &self.completions[..MAX_DISPLAY]
                ),
            );
        }
        s.field("flags", &self.flags).finish()
    }
}

impl ProgrammableCompleteReturn {
    pub fn new(completions: Vec<String>, flags: CompletionFlags) -> Self {
        Self { completions, flags }
    }

    pub fn from(
        completions: Vec<String>,
        quote_type: Option<QuoteType>,
        foundcs: c_int,
        append_char: i32,
    ) -> Self {
        Self::new(
            completions,
            CompletionFlags::from(quote_type, foundcs, append_char),
        )
    }
}

fn vec_of_strings_from_char_char_ptr(ptr: *mut *mut c_char) -> Vec<String> {
    let mut strings = Vec::new();
    let mut seen = HashSet::new();
    unsafe {
        if ptr.is_null() {
            return strings;
        }

        for i in 0.. {
            let c_str_ptr = *ptr.add(i);
            if c_str_ptr.is_null() {
                break;
            }
            let c_str = std::ffi::CStr::from_ptr(c_str_ptr);
            if let Ok(str_slice) = c_str.to_str()
                && seen.insert(str_slice)
            {
                strings.push(str_slice.to_string());
            }
        }
    }
    strings
}

pub fn run_programmable_completions(
    full_command: &str,                // "git commi asdf" with cursor just after com
    command_word: &str,                // "git"
    word_under_cursor: &str,           // "commi"
    cursor_byte_pos: usize,            // 7 since cursor is after "com" in "git com|mi asdf"
    word_under_cursor_byte_end: usize, // 9 since we want the end of "commi"
) -> Result<ProgrammableCompleteReturn> {
    log::debug!(
        "run_programmable_completions called with\nfull_command='{}'\ncommand_word='{}'\nword_under_cursor='{}'\ncursor_byte_pos={}\nword_under_cursor_byte_end={}",
        full_command,
        command_word,
        word_under_cursor,
        cursor_byte_pos,
        word_under_cursor_byte_end
    );

    if !full_command.starts_with(command_word) {
        log::debug!(
            "Command word '{}' not found in full command '{}'",
            command_word,
            full_command
        );
        return Err(anyhow::anyhow!(
            "Command word '{}' not found in full command '{}'",
            command_word,
            full_command
        ));
    }

    unsafe {
        let full_command_cstr = std::ffi::CString::new(full_command).unwrap();
        bash_symbols::rl_line_buffer = bash_symbols::xmalloc_cstr(&full_command_cstr); // git commi asdf
        bash_symbols::rl_point = cursor_byte_pos as std::ffi::c_int; // 7 ("git com|mi asdf")
        bash_symbols::set_readline_state(bash_symbols::RL_STATE_COMPLETING);

        let quote_type = find_quote_type(word_under_cursor);
        bash_symbols::rl_completion_quote_character =
            quote_type.map(|q| q.into_byte()).unwrap_or(0) as std::ffi::c_int;
        bash_symbols::rl_completion_found_quote = if quote_type.is_some() { 1 } else { 0 };
        bash_symbols::rl_filename_quoting_function = Some(quoting_function_c);
        bash_symbols::rl_filename_dequoting_function = Some(dequoting_function_c);
        // similar to set_completion_defaults
        bash_symbols::rl_filename_completion_desired = 0;
        bash_symbols::rl_filename_quoting_desired = 1;
        bash_symbols::rl_completion_suppress_append = 0;
        bash_symbols::rl_completion_append_character = ' ' as c_int;
        bash_symbols::rl_sort_completion_matches = 1;

        let foundcs: std::ffi::c_int = 0;

        let list_of_strs = bash_symbols::programmable_completions(
            std::ffi::CString::new(command_word).unwrap().as_ptr(),
            std::ffi::CString::new(word_under_cursor).unwrap().as_ptr(),
            0,
            word_under_cursor_byte_end as std::ffi::c_int,
            &foundcs as *const std::ffi::c_int as *mut std::ffi::c_int,
        );

        bash_symbols::clear_readline_state(bash_symbols::RL_STATE_COMPLETING);

        print_copt_flags(foundcs);

        if foundcs != 0 {
            // Copying logic from bashline.c:attempt_shell_completion
            // This is to pickup the filename desire from calls like `complete -o filenames`
            // This probably isn't necessary since I am reading the values from foundcs directly but it doesn't hurt to be safe
            bash_symbols::pcomp_set_readline_variables(foundcs, 1);
        }

        // The matches won't be escaped / quoted.
        let completion_strings = vec_of_strings_from_char_char_ptr(list_of_strs);
        // Readline also deduplicates the results
        let res = ProgrammableCompleteReturn::from(
            completion_strings,
            quote_type,
            foundcs,
            bash_symbols::rl_completion_append_character,
        );

        log::debug!(
            "Programmable completions found with foundcs={}: {:#?}",
            foundcs,
            res
        );

        if res.completions.is_empty() && res.flags.bash_default_fallback_desired {
            // Flyline used to support bash default completions as a fallback, but has deprecated
            // this in favor of flyline's own secondary completions.
            log::warn!(
                "Bash default completions requested by compspec, but flyline will try its own secondary completions instead."
            );
        } else {
            log::debug!(
                "Bash default fallback not desired or completions found. Returning programmable completions."
            );
        }

        Ok(res)
    }
}

pub fn print_copt_flags(flag: c_int) {
    log::debug!("COMPSPEC options flags set for flag {}:", flag);
    for option in &[
        CompspecOption::Reserved,
        CompspecOption::Default,
        CompspecOption::Filenames,
        CompspecOption::Dirnames,
        CompspecOption::NoQuote,
        CompspecOption::NoSpace,
        CompspecOption::BashDefault,
        CompspecOption::PlusDirs,
        CompspecOption::NoSort,
        CompspecOption::FullQuote,
    ] {
        if flag & (*option as c_int) != 0 {
            log::debug!(" - {:?}", option);
        }
    }
}

pub fn get_shell_var(var_name: &str) -> Option<ShellVar> {
    unsafe {
        let var_cstr = std::ffi::CString::new(var_name).unwrap();
        let value_ptr = bash_symbols::find_variable(var_cstr.as_ptr());
        if value_ptr.is_null() {
            return None;
        }
        Some((*value_ptr).clone())
    }
}

pub fn get_envvar_value(var_name: &str) -> Option<String> {
    get_shell_var(var_name).and_then(|var| var.get_value())
}

pub fn get_hostname() -> String {
    unsafe {
        let ptr = bash_symbols::current_host_name;
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

pub fn get_cwd() -> String {
    unsafe {
        let ptr = bash_symbols::get_working_directory(c"flyline".as_ptr());
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

pub fn expand_filename(filename: &str) -> String {
    unsafe {
        let expanded_string = bash_symbols::expand_string_to_string(
            std::ffi::CString::new(filename).unwrap().as_ptr(),
            0,
        );

        if expanded_string.is_null() {
            return filename.to_string();
        }

        let c_str = std::ffi::CStr::from_ptr(expanded_string);
        c_str
            .to_str()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_else(|| filename.to_string())
    }
}

pub fn fully_expand_path(p: &str) -> String {
    // p might have a tilde, env vars, and be relative
    // Use bash's own filename expansion ($VAR + ${VAR} + more).
    let bash_expanded = if p.is_empty() {
        String::new()
    } else {
        expand_filename(&dequoting_function_rust(p))
    };

    // Make the path absolute (prepend cwd when relative or empty).
    if bash_expanded.is_empty() {
        match std::env::current_dir() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(e) => {
                log::warn!("Failed to get current directory: {}", e);
                String::new()
            }
        }
    } else if !Path::new(&bash_expanded).is_absolute() {
        match std::env::current_dir() {
            Ok(p) => format!("{}/{}", p.display(), bash_expanded),
            Err(e) => {
                log::warn!("Failed to get current directory: {}", e);
                bash_expanded
            }
        }
    } else {
        bash_expanded
    }
}

// QuoteType can be  in the middle  of a word (i.e.  backslash)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote,
    #[default]
    Backslash,
}

impl QuoteType {
    pub fn from_char(c: char) -> Option<QuoteType> {
        match c {
            '\'' => Some(QuoteType::SingleQuote),
            '"' => Some(QuoteType::DoubleQuote),
            '\\' => Some(QuoteType::Backslash),
            _ => None,
        }
    }

    pub fn into_byte(self) -> u8 {
        match self {
            QuoteType::SingleQuote => b'\'',
            QuoteType::DoubleQuote => b'"',
            QuoteType::Backslash => b'\\',
        }
    }
}

/* Quote a filename using double quotes, single quotes, or backslashes
depending on the value of completion_quoting_style.  If we're
completing using backslashes, we need to quote some additional
characters (those that readline treats as word breaks), so we call
quote_word_break_chars on the result.  This returns newly-allocated
memory. */
// static char * bash_quote_filename (char *s, int rtype, char *qcp)
// TODO: handle edge cases that bash_quote_filename handles
extern "C" fn quoting_function_c(
    s: *const c_char,
    _rtype: c_int,
    quote_char: *const c_char,
) -> *mut c_char {
    let s_str = unsafe { std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned() };
    let quote_char_str = unsafe { std::ffi::CStr::from_ptr(quote_char).to_string_lossy() };
    let quote_type = quote_char_str
        .chars()
        .next()
        .and_then(QuoteType::from_char)
        .unwrap_or_default();
    let quoted = quote_function_rust(&s_str, quote_type);
    let quoted_cstr = std::ffi::CString::new(quoted).unwrap();
    unsafe { bash_symbols::xmalloc_cstr(&quoted_cstr) }
}

pub fn quote_function_rust(s: &str, quote_type: QuoteType) -> String {
    match quote_type {
        QuoteType::SingleQuote => format!("'{}'", s.replace('\'', "'\\''")),
        QuoteType::DoubleQuote => {
            let escaped: String = s
                .chars()
                .map(|c| {
                    if DOUBLE_QUOTE_SPECIAL_CHARS.contains(&c) {
                        format!("\\{}", c)
                    } else {
                        c.to_string()
                    }
                })
                .collect();

            format!("\"{}\"", escaped)
        }
        QuoteType::Backslash => s
            .chars()
            .map(|c| {
                if c.is_whitespace() || BACKSLASH_SPECIAL_CHARS.contains(&c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            })
            .collect(),
    }
}

const DOUBLE_QUOTE_SPECIAL_CHARS: &[char] = &['$', '`', '"', '\\', '!', '\n'];
const BACKSLASH_SPECIAL_CHARS: &[char] = &[
    ' ', '\t', '\n', '\\', '"', '\'', '!', '$', '&', '(', ')', '*', ';', '<', '>', '?', '[', ']',
    '^', '`', '{', '|', '}',
];

/* Filename quoting for completion. */
/* A function to strip unquoted quote characters (single quotes, double
quotes, and backslashes).  It allows single quotes to appear
within double quotes, and vice versa.  It should be smarter. */
// static char *bash_dequote_filename (char *text, int quote_char)
extern "C" fn dequoting_function_c(s: *const c_char, _quote_char: c_int) -> *mut c_char {
    let s_str = unsafe { std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned() };
    let dequoted = dequoting_function_rust(&s_str);
    let dequoted_cstr = std::ffi::CString::new(dequoted).unwrap();
    unsafe { bash_symbols::xmalloc_cstr(&dequoted_cstr) }
}

pub fn dequoting_function_rust(s: &str) -> String {
    let mut res = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next_char) = chars.next() {
                    res.push(next_char);
                }
            }
            '\'' => {
                for next_char in chars.by_ref() {
                    if next_char == '\'' {
                        break;
                    }
                    res.push(next_char);
                }
            }
            '"' => {
                while let Some(next_char) = chars.next() {
                    if next_char == '"' {
                        break;
                    }
                    if next_char == '\\' {
                        if let Some(escaped_char) = chars.next() {
                            res.push(escaped_char);
                        }
                    } else {
                        res.push(next_char);
                    }
                }
            }
            _ => res.push(c),
        }
    }
    res
}

// This function
//    returns the opening quote character if we found an unclosed quoted
//    substring, '\0' otherwise.  FP, if non-null, is set to a value saying
//    which (shell-like) quote characters we found (single quote, double
//    quote, or backslash) anywhere in the string.  DP, if non-null, is set to
//    the value of the delimiter character that caused a word break. */
// It sets fp to  a bitfield  but no one ever reads that bitfield so we can ignore it for now
// char _rl_find_completion_word (int *fp, int *dp)

pub fn find_quote_type(s: &str) -> Option<QuoteType> {
    if s.starts_with("\"") {
        return Some(QuoteType::DoubleQuote);
    } else if s.starts_with('\'') {
        return Some(QuoteType::SingleQuote);
    } else {
        // Check for odd number of consecutive backslashes
        let mut backslash_count = 0;
        let mut max_consecutive_backslashes = 0;

        for c in s.chars() {
            if c == '\\' {
                backslash_count += 1;
            } else if backslash_count > 0 {
                max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
                backslash_count = 0;
            }
        }
        // Handle case where string ends with backslashes
        if backslash_count > 0 {
            max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
        }

        if max_consecutive_backslashes > 0 && max_consecutive_backslashes % 2 == 1 {
            return Some(QuoteType::Backslash);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Cached environment lookups (moved from BashEnvManager)
// ---------------------------------------------------------------------------

static DEFINED_ALIASES: LazyLock<Vec<String>> = LazyLock::new(get_all_aliases);
static DEFINED_RESERVED_WORDS: LazyLock<Vec<String>> = LazyLock::new(get_all_reserved_words);
static DEFINED_SHELL_FUNCTIONS: LazyLock<Vec<String>> = LazyLock::new(get_all_shell_functions);
static DEFINED_BUILTINS: LazyLock<Vec<String>> = LazyLock::new(get_all_shell_builtins);
static DEFINED_EXECUTABLES: LazyLock<Vec<(PathBuf, String)>> = LazyLock::new(|| {
    if let Some(path_str) = get_envvar_value("PATH") {
        get_executables_from_path(&path_str)
    } else {
        Vec::new()
    }
});
static LS_COLORS: LazyLock<Option<LsColors>> =
    LazyLock::new(|| get_envvar_value("LS_COLORS").map(|s| LsColors::from_string(&s)));

/// Return a ratatui `Style` for the given path based on the `LS_COLORS` environment variable.
/// Returns `None` if `LS_COLORS` was not set or the path has no matching entry.
pub fn style_for_path(path: &Path) -> Option<Style> {
    let lscolors_style = LS_COLORS.as_ref()?.style_for_path(path)?;
    Some(lscolors_style_to_ratatui(lscolors_style))
}

/// Get all potential first word completions (aliases, reserved words, functions, builtins, executables)
pub fn get_first_word_completions(command: &str) -> Vec<String> {
    let mut res = Vec::new();
    let mut seen = HashSet::new();

    if command.is_empty() {
        return res;
    }

    for poss_completion in DEFINED_ALIASES
        .iter()
        .chain(DEFINED_RESERVED_WORDS.iter())
        .chain(DEFINED_SHELL_FUNCTIONS.iter())
        .chain(DEFINED_BUILTINS.iter())
        .chain(DEFINED_EXECUTABLES.iter().map(|(_, name)| name))
    {
        if poss_completion.starts_with(command) && seen.insert(poss_completion.as_str()) {
            res.push(poss_completion.to_string());
        }
    }

    res
}

/// Get fuzzy first word completions using ArinaeMatcher for when no exact prefix match is found
pub fn get_fuzzy_first_word_completions(command: &str) -> Vec<String> {
    if command.is_empty() {
        return vec![];
    }

    let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);
    let mut scored: Vec<(i64, String)> = DEFINED_ALIASES
        .iter()
        .chain(DEFINED_RESERVED_WORDS.iter())
        .chain(DEFINED_SHELL_FUNCTIONS.iter())
        .chain(DEFINED_BUILTINS.iter())
        .chain(DEFINED_EXECUTABLES.iter().map(|(_, name)| name))
        .filter_map(|poss_completion| {
            matcher
                .fuzzy_match(poss_completion, command)
                .map(|score| (score, poss_completion.to_string()))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, s)| s).collect()
}

fn get_executables_from_path(path_str: &str) -> Vec<(PathBuf, String)> {
    let mut executables = Vec::new();
    for path_dir in path_str.split(':') {
        if let Ok(entries) = std::fs::read_dir(path_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata()
                    && metadata.is_file()
                {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        if let Some(file_name) = entry.file_name().to_str() {
                            executables.push((entry.path(), file_name.to_string()));
                        }
                    }
                }
            }
        }
    }
    executables
}

/// Convert an `lscolors::Color` to a `ratatui::style::Color`.
fn lscolors_color_to_ratatui(color: lscolors::Color) -> Color {
    match color {
        lscolors::Color::Black => Color::Black,
        lscolors::Color::Red => Color::Red,
        lscolors::Color::Green => Color::Green,
        lscolors::Color::Yellow => Color::Yellow,
        lscolors::Color::Blue => Color::Blue,
        lscolors::Color::Magenta => Color::Magenta,
        lscolors::Color::Cyan => Color::Cyan,
        lscolors::Color::White => Color::White,
        lscolors::Color::BrightBlack => Color::DarkGray,
        lscolors::Color::BrightRed => Color::LightRed,
        lscolors::Color::BrightGreen => Color::LightGreen,
        lscolors::Color::BrightYellow => Color::LightYellow,
        lscolors::Color::BrightBlue => Color::LightBlue,
        lscolors::Color::BrightMagenta => Color::LightMagenta,
        lscolors::Color::BrightCyan => Color::LightCyan,
        lscolors::Color::BrightWhite => Color::Gray,
        lscolors::Color::Fixed(n) => Color::Indexed(n),
        lscolors::Color::RGB(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Convert an `lscolors::Style` to a `ratatui::style::Style`.
fn lscolors_style_to_ratatui(style: &lscolors::Style) -> Style {
    let mut ratatui_style = Style::default();

    if let Some(fg) = style.foreground {
        ratatui_style = ratatui_style.fg(lscolors_color_to_ratatui(fg));
    }
    if let Some(bg) = style.background {
        ratatui_style = ratatui_style.bg(lscolors_color_to_ratatui(bg));
    }

    let fs = &style.font_style;
    if fs.bold {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if fs.dimmed {
        ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
    }
    if fs.italic {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if fs.underline {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }
    if fs.slow_blink {
        ratatui_style = ratatui_style.add_modifier(Modifier::SLOW_BLINK);
    }
    if fs.rapid_blink {
        ratatui_style = ratatui_style.add_modifier(Modifier::RAPID_BLINK);
    }
    if fs.reverse {
        ratatui_style = ratatui_style.add_modifier(Modifier::REVERSED);
    }
    if fs.hidden {
        ratatui_style = ratatui_style.add_modifier(Modifier::HIDDEN);
    }
    if fs.strikethrough {
        ratatui_style = ratatui_style.add_modifier(Modifier::CROSSED_OUT);
    }

    ratatui_style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_function() {
        assert_eq!(
            quote_function_rust(r#"qwe asd"#, QuoteType::Backslash),
            r#"qwe\ asd"#
        );
        assert_eq!(
            quote_function_rust(r#"qwe asd"#, QuoteType::DoubleQuote),
            r#""qwe asd""#
        );
        assert_eq!(
            quote_function_rust(r#"qwe asd"#, QuoteType::SingleQuote),
            r#"'qwe asd'"#
        );
    }

    #[test]
    fn test_quote_function_harder() {
        assert_eq!(
            quote_function_rust(r#"qwe"asdf"#, QuoteType::Backslash),
            r#"qwe\"asdf"#
        );
        assert_eq!(
            quote_function_rust(r#"qwe"asdf"#, QuoteType::DoubleQuote),
            r#""qwe\"asdf""#
        );
    }

    #[test]
    fn test_quote_function_backslash_special_chars() {
        for &c in BACKSLASH_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected = format!("a\\{}b", c);
            assert_eq!(quote_function_rust(&input, QuoteType::Backslash), expected);
        }
    }

    #[test]
    fn test_quote_function_double_quote_special_chars() {
        for &c in DOUBLE_QUOTE_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected_inner = format!("a\\{}b", c);
            let expected = format!("\"{}\"", expected_inner);
            assert_eq!(
                quote_function_rust(&input, QuoteType::DoubleQuote),
                expected
            );
        }
    }

    #[test]
    fn test_dequoting_function() {
        assert_eq!(dequoting_function_rust(r#"qwe\ asd"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#""qwe asd""#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"'qwe asd'"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"abc"#), r#"abc"#);
    }

    #[test]
    fn test_dequoting_function_harder() {
        assert_eq!(dequoting_function_rust(r#"qwe\"asdf"#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""qwe\"asdf""#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""""#), r#""#);
    }

    #[test]
    fn test_find_quotes() {
        assert_eq!(
            find_quote_type(r#""qwe asdf"#),
            Some(QuoteType::DoubleQuote)
        );
        assert_eq!(
            find_quote_type(r#"'qwe asdf"#),
            Some(QuoteType::SingleQuote)
        );
        assert_eq!(find_quote_type(r#"qwe\ asdf"#), Some(QuoteType::Backslash));
        assert_eq!(find_quote_type(r#"qwe asdf"#), None);
        assert_eq!(find_quote_type(r#"qwe\\asdf"#), None);
    }
}
