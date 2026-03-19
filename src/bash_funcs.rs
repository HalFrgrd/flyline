use crate::bash_symbols;

use anyhow::Result;

use libc::{c_char, c_int};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Read;
use std::os::unix::io::FromRawFd;
use std::sync::Mutex;
use std::sync::OnceLock;

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
    // Call the `type` builtin to check if the command exists
    let cmd_c_str = std::ffi::CString::new(cmd).unwrap();

    let (_, command_type_output) = with_redirected_stdout(|| unsafe {
        bash_symbols::describe_command(cmd_c_str.as_ptr(), bash_symbols::CDescFlag::Type as c_int)
    });
    let command_type = CommandType::from_str(&command_type_output.trim());

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

pub fn get_command_info(cmd: &str) -> (CommandType, String) {
    static CALL_TYPE_CACHE: OnceLock<Mutex<HashMap<String, (CommandType, String)>>> =
        OnceLock::new();
    CALL_TYPE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut cache = CALL_TYPE_CACHE.get().unwrap().lock().unwrap();

    if let Some(res) = cache.get(cmd) {
        res.clone()
    } else {
        let result = get_command_type_uncached(cmd);
        cache.insert(cmd.to_string(), result.clone());
        result
    }
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
    return vec![
        "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until",
        "do", "done", "in", "function", "time", "{", "}", "!", "[[", "]]", "coproc",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
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

#[derive(Debug)]
pub struct ProgrammableCompleteReturn {
    pub completions: Vec<String>,
    pub flags: CompletionFlags,
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
            if let Ok(str_slice) = c_str.to_str() {
                if seen.insert(str_slice) {
                    strings.push(str_slice.to_string());
                }
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
        bash_symbols::rl_line_buffer = std::ffi::CString::new(full_command).unwrap().into_raw(); // git commi asdf
        bash_symbols::rl_point = cursor_byte_pos as std::ffi::c_int; // 7 ("git com|mi asdf")
        bash_symbols::rl_readline_state |= 0x00004000; // RL_STATE_COMPLETING

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
            // vec!["$HOME/qwe asd".to_string()],
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

pub fn get_env_variable(var_name: &str) -> Option<String> {
    unsafe {
        let var_cstr = std::ffi::CString::new(var_name).unwrap();
        let value_ptr = bash_symbols::getenv(var_cstr.as_ptr());
        if value_ptr.is_null() {
            return None;
        }
        let c_str = std::ffi::CStr::from_ptr(value_ptr);
        c_str.to_str().ok().map(|s| s.to_string())
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

// QuoteType can be  in the middle  of a word (i.e.  backslash)
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote,
    Backslash,
}

impl Default for QuoteType {
    fn default() -> Self {
        QuoteType::Backslash
    }
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

    pub fn into_byte(&self) -> u8 {
        match self {
            QuoteType::SingleQuote => '\'' as u8,
            QuoteType::DoubleQuote => '"' as u8,
            QuoteType::Backslash => '\\' as u8,
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
    std::ffi::CString::new(quoted).unwrap().into_raw()
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
    std::ffi::CString::new(dequoted).unwrap().into_raw()
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
                while let Some(next_char) = chars.next() {
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
            } else {
                if backslash_count > 0 {
                    max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
                    backslash_count = 0;
                }
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
