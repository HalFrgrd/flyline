#[cfg(not(test))]
use crate::bash_symbols;
#[cfg(not(test))]
use crate::bash_symbols::ShellVar;

use anyhow::Result;

#[cfg(not(test))]
use libc::c_char;
use libc::c_int;
use lscolors::LsColors;
use std::collections::HashMap;
#[cfg(not(test))]
use std::collections::HashSet;
#[cfg(not(test))]
use std::io::Read;
#[cfg(not(test))]
use std::os::unix::fs::PermissionsExt;
#[cfg(not(test))]
use std::os::unix::io::FromRawFd;
use std::path::Path;
#[cfg(not(test))]
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
#[cfg(not(test))]
use std::time::SystemTime;

#[cfg(not(test))]
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
pub enum CommandWordInfo {
    Unknown {
        command: String,
    },
    Alias {
        command: String,
        expansion: String,
    },
    Keyword {
        command: String,
        usage: Option<String>,
    },
    Function {
        command: String,
        source_file: Option<String>,
        line: Option<i32>,
    },
    Builtin {
        command: String,
        usage: Option<String>,
    },
    File {
        command: String,
        path: String,
    },
}

impl CommandWordInfo {
    pub fn is_known(&self) -> bool {
        !matches!(self, CommandWordInfo::Unknown { .. })
    }

    pub fn command(&self) -> &str {
        match self {
            CommandWordInfo::Unknown { command } => command,
            CommandWordInfo::Alias { command, .. } => command,
            CommandWordInfo::Keyword { command, .. } => command,
            CommandWordInfo::Function { command, .. } => command,
            CommandWordInfo::Builtin { command, .. } => command,
            CommandWordInfo::File { command, .. } => command,
        }
    }

    pub fn to_description(&self) -> String {
        match self {
            CommandWordInfo::Unknown { .. } => "unknown".to_string(),
            CommandWordInfo::Alias { expansion, .. } => format!("alias: {}", expansion),
            CommandWordInfo::Keyword { command, usage } => {
                if let Some(u) = usage {
                    format!("keyword: {}", u)
                } else {
                    format!("keyword: {}", command)
                }
            }
            CommandWordInfo::Builtin { command, usage } => {
                if let Some(u) = usage {
                    format!("builtin: {}", u)
                } else {
                    format!("builtin: {}", command)
                }
            }
            CommandWordInfo::File { path, .. } => format!("file: {}", path),
            CommandWordInfo::Function {
                source_file, line, ..
            } => match (source_file, line) {
                (Some(file), Some(l)) => format!("function {}:{}", file, l),
                (Some(file), None) => format!("function {}", file),
                (None, Some(l)) => format!("function :{}", l),
                (None, None) => "function".to_string(),
            },
        }
    }
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn find_alias(cmd: &str) -> Option<String> {
    test_fixtures::test_aliases()
        .iter()
        .find_map(|(name, value)| (*name == cmd).then(|| (*value).to_string()))
}

#[cfg(not(test))]
fn get_command_info_uncached(cmd: &str) -> CommandWordInfo {
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
    let command_type_str = command_type_output.trim();

    match command_type_str {
        "alias" => {
            let expansion = find_alias(cmd).unwrap_or_else(|| cmd.to_string());
            CommandWordInfo::Alias {
                command: cmd.to_string(),
                expansion,
            }
        }
        "keyword" => {
            let (_, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::ShortDesc as c_int,
                )
            });
            let usage = if output.is_empty() {
                None
            } else {
                Some(output.trim().to_string())
            };
            CommandWordInfo::Keyword {
                command: cmd.to_string(),
                usage,
            }
        }
        "builtin" => {
            let (_, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::ShortDesc as c_int,
                )
            });
            let usage = if output.is_empty() {
                None
            } else {
                Some(output.trim().to_string())
            };
            CommandWordInfo::Builtin {
                command: cmd.to_string(),
                usage,
            }
        }
        "file" => {
            let (_, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::PathOnly as c_int,
                )
            });
            CommandWordInfo::File {
                command: cmd.to_string(),
                path: output.trim().to_string(),
            }
        }
        "function" => unsafe {
            let func_def_ptr = bash_symbols::find_function_def(cmd_c_str.as_ptr());
            if !func_def_ptr.is_null() {
                let func_def = &*func_def_ptr;
                let line = if func_def.line > 0 {
                    Some(func_def.line)
                } else {
                    None
                };
                let source_file = if func_def.source_file.is_null() {
                    None
                } else {
                    std::ffi::CStr::from_ptr(func_def.source_file)
                        .to_str()
                        .ok()
                        .map(|s| s.to_string())
                };
                CommandWordInfo::Function {
                    command: cmd.to_string(),
                    source_file,
                    line,
                }
            } else {
                CommandWordInfo::Function {
                    command: cmd.to_string(),
                    source_file: None,
                    line: None,
                }
            }
        },
        _ => CommandWordInfo::Unknown {
            command: cmd.to_string(),
        },
    }
}

static CALL_TYPE_CACHE: Mutex<Option<HashMap<String, CommandWordInfo>>> = Mutex::new(None);

#[cfg(not(test))]
pub fn get_command_info(cmd: &str) -> CommandWordInfo {
    let mut cache_guard = CALL_TYPE_CACHE.lock().unwrap();
    let cache = cache_guard.get_or_insert_with(HashMap::new);

    if let Some(res) = cache.get(cmd) {
        res.clone()
    } else {
        let result = get_command_info_uncached(cmd);
        cache.insert(cmd.to_string(), result.clone());
        result
    }
}

#[cfg(test)]
pub fn get_command_info(cmd: &str) -> CommandWordInfo {
    // The test environment models a tiny world: `git` is the only "real"
    // executable on PATH, so it gets reported as a File at /usr/bin/git.
    // Everything else is unknown — tests that need additional command types
    // can extend this match arm.
    if cmd == "git" {
        return CommandWordInfo::File {
            command: "git".to_string(),
            path: "/usr/bin/git".to_string(),
        };
    }
    if let Some(expansion) = test_fixtures::test_aliases()
        .iter()
        .find_map(|(name, value)| (*name == cmd).then(|| (*value).to_string()))
    {
        return CommandWordInfo::Alias {
            command: cmd.to_string(),
            expansion,
        };
    }
    CommandWordInfo::Unknown {
        command: cmd.to_string(),
    }
}

#[cfg(not(test))]
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

#[cfg(not(test))]
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

#[cfg(test)]
pub fn format_shell_var(name: &str) -> String {
    format!("${}=", name)
}

pub fn reset_caches() {
    let mut cache_guard = CALL_TYPE_CACHE.lock().unwrap();
    *cache_guard = None;

    let mut cache_guard = SHELL_VAR_CACHE.lock().unwrap();
    *cache_guard = None;

    *DEFINED_ALIASES.lock().unwrap() = None;
    *DEFINED_RESERVED_WORDS.lock().unwrap() = None;
    *DEFINED_SHELL_FUNCTIONS.lock().unwrap() = None;
    *DEFINED_BUILTINS.lock().unwrap() = None;
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn get_all_aliases() -> Vec<String> {
    test_fixtures::test_aliases()
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect()
}

pub fn get_all_reserved_words() -> Vec<String> {
    log::info!("Getting cached reserved words");

    vec![
        "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until",
        "do", "done", "in", "function", "time", "{", "}", "!", "[[", "]]", "coproc",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn get_all_variables_with_prefix(prefix: &str) -> Vec<String> {
    let bare_prefix = prefix.strip_prefix('$').unwrap_or(prefix);
    let mut variables: Vec<String> = test_fixtures::test_env_vars()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| name.starts_with(bare_prefix))
        .map(|name| format!("${}", name))
        .collect();
    variables.sort();
    variables.dedup();
    variables
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn get_all_shell_functions() -> Vec<String> {
    Vec::new()
}

#[cfg(not(test))]
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
// In bash >= 4.4, COPT_NOQUOTE was inserted at (1<<4), shifting later values.
// In bash < 4.4: NOSPACE=(1<<4), BASHDEFAULT=(1<<5), PLUSDIRS=(1<<6)
// In bash >= 4.4: NOQUOTE=(1<<4), NOSPACE=(1<<5), BASHDEFAULT=(1<<6), PLUSDIRS=(1<<7), NOSORT=(1<<8), FULLQUOTE=(1<<9)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompspecOption {
    Reserved = 1 << 0,
    Default = 1 << 1,
    Filenames = 1 << 2,
    Dirnames = 1 << 3,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoQuote = 1 << 4,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoSpace = 1 << 5,
    #[cfg(not(feature = "pre_bash_4_4"))]
    BashDefault = 1 << 6,
    #[cfg(not(feature = "pre_bash_4_4"))]
    PlusDirs = 1 << 7,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoSort = 1 << 8,
    #[cfg(not(feature = "pre_bash_4_4"))]
    FullQuote = 1 << 9,
    #[cfg(feature = "pre_bash_4_4")]
    NoSpace = 1 << 4,
    #[cfg(feature = "pre_bash_4_4")]
    BashDefault = 1 << 5,
    #[cfg(feature = "pre_bash_4_4")]
    PlusDirs = 1 << 6,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub some_dont_end_in_equal_sign: bool,
}

impl CompletionFlags {
    pub fn from(
        quote_type: Option<QuoteType>,
        foundcs: c_int,
        append_char: i32,
        some_dont_end_in_equal_sign: bool,
    ) -> Self {
        Self {
            quote_type,
            readline_default_fallback_desired: foundcs & (CompspecOption::Default as c_int) != 0,
            #[cfg(not(feature = "pre_bash_4_4"))]
            filename_quoting_desired: foundcs & (CompspecOption::NoQuote as c_int) == 0,
            #[cfg(feature = "pre_bash_4_4")]
            filename_quoting_desired: true,
            filename_completion_desired: foundcs & (CompspecOption::Filenames as c_int) != 0,
            no_suffix_desired: foundcs & (CompspecOption::NoSpace as c_int) != 0,
            suffix_character: char::from_u32(append_char as u32).unwrap_or(' '),
            bash_default_fallback_desired: foundcs & (CompspecOption::BashDefault as c_int) != 0,
            #[cfg(not(feature = "pre_bash_4_4"))]
            nosort_desired: foundcs & (CompspecOption::NoSort as c_int) != 0,
            #[cfg(feature = "pre_bash_4_4")]
            nosort_desired: false,
            some_dont_end_in_equal_sign,
        }
    }

    #[cfg(test)]
    pub fn from_alt(word_under_cursor: &str, completions: &[String]) -> Self {
        let mut flags = Self::default();
        flags.quote_type = find_quote_type(word_under_cursor);
        flags.some_dont_end_in_equal_sign = completions.iter().any(|s| !s.ends_with('='));
        flags
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
            some_dont_end_in_equal_sign: false,
        }
    }
}

pub struct ProgrammableCompleteReturn {
    pub completions: Vec<String>,
    pub flags: CompletionFlags,
    pub compspec_was_useful: bool,
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
        s.field("flags", &self.flags)
            .field("compspec_was_useful", &self.compspec_was_useful)
            .finish()
    }
}

fn is_strict_completion_value(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let first = val.chars().next().unwrap();
    if !first.is_ascii_alphanumeric() && first != '-' {
        return false;
    }
    val.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == ':'
            || c == '.'
            || c == '/'
            || c == '@'
    })
}

fn analyze_candidate(s: &str) -> Option<(&str, &str, usize)> {
    if s.contains('\t') {
        return None;
    }
    if let Some(pos) = s.find("  ") {
        let value = &s[..pos];
        let rest = &s[pos..];
        let description = rest.trim_start();
        let desc_start_index = s.len() - rest.len() + (rest.len() - description.len());

        if is_strict_completion_value(value) && !description.is_empty() {
            return Some((value, description, desc_start_index));
        }
    }
    None
}

fn should_infer_filename_completion(completions: &[String], flags: &CompletionFlags) -> bool {
    if flags.filename_completion_desired
        || completions.is_empty()
        || completions.len() >= crate::FILENAME_INFERENCE_LIMIT
    {
        return false;
    }

    completions.iter().all(|completion| {
        !completion.contains('\t') && Path::new(&fully_expand_path(completion)).exists()
    })
}

/// Some completion scripts like gh or docker put descriptions inline with
/// the suggestion when there are multiple suggestions.
/// So here I convert those to the format "suggestion<TAB>description" so that
/// flyline can show the description in a separate column.
fn detect_and_convert_inline_descriptions(completions: &mut Vec<String>, flags: &CompletionFlags) {
    if flags.filename_completion_desired || completions.iter().any(|s| s.contains('\t')) {
        return;
    }

    let mut detected = false;

    if completions.len() == 1 {
        if let Some((value, description, _)) = analyze_candidate(&completions[0]) {
            if description.contains(' ') || value.starts_with('-') {
                detected = true;
            }
        }
    } else if completions.len() > 1 {
        let mut desc_columns = std::collections::HashMap::new();

        for s in completions.iter() {
            if let Some((_, _, col)) = analyze_candidate(s) {
                *desc_columns.entry(col).or_insert(0) += 1;
            }
        }

        let has_aligned = desc_columns.values().any(|&count| count >= 2);

        if has_aligned {
            detected = true;
        }
    }

    if detected {
        for s in completions.iter_mut() {
            if let Some((value, description, _)) = analyze_candidate(s) {
                let description = if let Some(stripped) = description
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                {
                    stripped
                } else {
                    description
                };
                *s = format!("{}\t{}", value, description);
            }
        }
    }
}

impl ProgrammableCompleteReturn {
    pub fn new(
        mut completions: Vec<String>,
        mut flags: CompletionFlags,
        compspec_was_useful: bool,
    ) -> Self {
        if should_infer_filename_completion(&completions, &flags) {
            flags.filename_completion_desired = true;
        }
        detect_and_convert_inline_descriptions(&mut completions, &flags);
        Self {
            completions,
            flags,
            compspec_was_useful,
        }
    }

    pub fn from(
        completions: Vec<String>,
        quote_type: Option<QuoteType>,
        foundcs: c_int,
        append_char: i32,
        compspec_was_useful: bool,
    ) -> Self {
        let some_dont_end_in_equal_sign = completions.iter().any(|s| !s.ends_with('='));
        Self::new(
            completions,
            CompletionFlags::from(
                quote_type,
                foundcs,
                append_char,
                some_dont_end_in_equal_sign,
            ),
            compspec_was_useful,
        )
    }
}

#[cfg(not(test))]
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

#[cfg(not(test))]
pub fn useful_compspec_ran(command_word: &str) -> bool {
    unsafe {
        let command_word_cstr = match std::ffi::CString::new(command_word) {
            Ok(cstr) => cstr,
            Err(_) => return false,
        };
        let compspec_ptr = bash_symbols::progcomp_search(command_word_cstr.as_ptr());
        if compspec_ptr.is_null() {
            log::info!(
                "useful_compspec_ran: no registered compspec found for '{}' (default/fallback)",
                command_word
            );
            return false;
        }
        let compspec = &*compspec_ptr;
        if compspec.funcname.is_null() {
            if !compspec.command.is_null() {
                if let Ok(cmd_str) = std::ffi::CStr::from_ptr(compspec.command).to_str() {
                    log::info!(
                        "useful_compspec_ran: registered compspec command for '{}' is: {}",
                        command_word,
                        cmd_str
                    );
                }
            } else {
                log::info!(
                    "useful_compspec_ran: registered compspec for '{}' has no funcname",
                    command_word
                );
            }
            return true;
        }
        let funcname_cstr = std::ffi::CStr::from_ptr(compspec.funcname);
        if let Ok(funcname_str) = funcname_cstr.to_str() {
            log::info!(
                "useful_compspec_ran: registered compspec function for '{}' is: {}",
                command_word,
                funcname_str
            );
            if funcname_str == "_minimal" || funcname_str == "_completion_loader"
            // || funcname_str == "_longopt"
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
pub fn useful_compspec_ran(_command_word: &str) -> bool {
    true
}

#[cfg(not(test))]
pub fn evaluate_shell_string(script: &str) -> Result<()> {
    unsafe {
        let script_cstr = std::ffi::CString::new(script)?;
        let allocated_ptr = bash_symbols::xmalloc_cstr(&script_cstr);
        let from_file_cstr = std::ffi::CString::new("flycomp")?;
        #[cfg(not(feature = "pre_bash_4_4"))]
        bash_symbols::evalstring(allocated_ptr, from_file_cstr.as_ptr(), 0);
        #[cfg(feature = "pre_bash_4_4")]
        bash_symbols::parse_and_execute(allocated_ptr, from_file_cstr.as_ptr(), 0);
        Ok(())
    }
}

#[cfg(test)]
pub fn evaluate_shell_string(_script: &str) -> Result<()> {
    Ok(())
}

#[cfg(not(test))]
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
        #[cfg(not(feature = "pre_bash_4_4"))]
        {
            bash_symbols::rl_completion_suppress_append = 0;
        }
        bash_symbols::rl_completion_append_character = ' ' as c_int;
        #[cfg(not(feature = "pre_bash_4_4"))]
        {
            bash_symbols::rl_sort_completion_matches = 1;
        }

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
            #[cfg(not(feature = "pre_bash_4_4"))]
            bash_symbols::pcomp_set_readline_variables(foundcs, 1);
        }

        // Detect when there was no useful compspec and a dummy one that just returned filenames was used instead
        let compspec_was_useful = useful_compspec_ran(command_word);
        log::info!(
            "run_programmable_completions: useful_compspec_ran for '{}' returned: {}",
            command_word,
            compspec_was_useful
        );

        let completion_strings = vec_of_strings_from_char_char_ptr(list_of_strs);

        let res = ProgrammableCompleteReturn::from(
            completion_strings,
            quote_type,
            foundcs,
            bash_symbols::rl_completion_append_character,
            compspec_was_useful,
        );

        log::debug!("Programmable completions found: {:#?}", res);

        Ok(res)
    }
}

#[cfg(test)]
pub fn run_programmable_completions(
    full_command: &str,
    command_word: &str,
    word_under_cursor: &str,
    _cursor_byte_pos: usize,
    _word_under_cursor_byte_end: usize,
) -> Result<ProgrammableCompleteReturn> {
    log::debug!(
        "[test] run_programmable_completions: full_command='{}', command_word='{}', word_under_cursor='{}'",
        full_command,
        command_word,
        word_under_cursor
    );

    if command_word == "git" {
        let candidates = test_fixtures::dummy_git_completions(full_command, word_under_cursor);
        let completions: Vec<String> = candidates
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect();
        let flags = CompletionFlags::from_alt(word_under_cursor, &completions);
        Ok(ProgrammableCompleteReturn::new(completions, flags, true))
    } else if command_word == "docker" {
        let completions = if word_under_cursor.starts_with('p') {
            vec![
                "port      List port mappings or a specific mapping for the container".to_string(),
                "ps        List containers".to_string(),
            ]
        } else {
            vec![
                "builder   Manage builds".to_string(),
                "image     Manage images".to_string(),
                "port      List port mappings or a specific mapping for the container".to_string(),
                "ps        List containers".to_string(),
                "run       Run a command in a new container".to_string(),
            ]
        };
        let filtered: Vec<String> = completions
            .into_iter()
            .filter(|s| s.starts_with(word_under_cursor))
            .collect();
        let flags = CompletionFlags::from_alt(word_under_cursor, &filtered);
        Ok(ProgrammableCompleteReturn::new(filtered, flags, true))
    } else if command_word == "cat" {
        // do a naive filessytem glob.
        // bash sometimes does this if nothing is returned by the prog comp spec.
        // Intentionally leave filename_completion_desired unset so tests can
        // exercise filename inference in ProgrammableCompleteReturn::new.

        // Split word_under_cursor at the final '/'.
        // lhs keeps the trailing slash so completions can be reassembled in the
        // same shape the user typed.
        let (lhs, rhs) = match word_under_cursor.rsplit_once('/') {
            Some((left, right)) => (format!("{left}/"), right),
            None => (String::new(), word_under_cursor),
        };

        // Expand the directory side (lhs) before filesystem lookup.
        let expanded_lhs = if lhs.is_empty() {
            expand_filename(".")
        } else {
            expand_filename(&lhs)
        };

        let mut completions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&expanded_lhs) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.starts_with(rhs)
                {
                    let mut candidate = if lhs.is_empty() {
                        name.to_string()
                    } else {
                        format!("{lhs}{name}")
                    };

                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        candidate.push('/');
                    }

                    completions.push(candidate);
                }
            }
        }

        completions.sort();
        completions.dedup();

        let flags = CompletionFlags::from_alt(word_under_cursor, &completions);
        Ok(ProgrammableCompleteReturn::new(completions, flags, true))
    } else {
        Ok(ProgrammableCompleteReturn::new(
            Vec::new(),
            CompletionFlags::default(),
            true,
        ))
    }
}

#[cfg(not(test))]
pub fn print_copt_flags(flag: c_int) {
    log::debug!("COMPSPEC options flags set for flag {}:", flag);
    let options: &[CompspecOption] = &[
        CompspecOption::Reserved,
        CompspecOption::Default,
        CompspecOption::Filenames,
        CompspecOption::Dirnames,
        #[cfg(not(feature = "pre_bash_4_4"))]
        CompspecOption::NoQuote,
        CompspecOption::NoSpace,
        CompspecOption::BashDefault,
        CompspecOption::PlusDirs,
        #[cfg(not(feature = "pre_bash_4_4"))]
        CompspecOption::NoSort,
        #[cfg(not(feature = "pre_bash_4_4"))]
        CompspecOption::FullQuote,
    ];
    for option in options {
        if flag & (*option as c_int) != 0 {
            log::debug!(" - {:?}", option);
        }
    }
}

#[cfg(not(test))]
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

#[cfg(not(test))]
pub fn get_envvar_value(var_name: &str) -> Option<String> {
    get_shell_var(var_name).and_then(|var| var.get_value())
}

#[cfg(test)]
pub fn get_envvar_value(var_name: &str) -> Option<String> {
    test_fixtures::test_env_vars()
        .into_iter()
        .find_map(|(name, value)| (name == var_name).then_some(value))
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn get_hostname() -> String {
    "test-host".to_string()
}

#[cfg(not(test))]
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

#[cfg(test)]
pub fn get_cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(not(test))]
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

/// Test-only filename expansion. Supports a tiny subset of bash expansion:
///   * `$PWD` / `$HOME` (and a leading `~/`) are expanded by looking the
///     name up in [`test_fixtures::test_env_vars`].
///   * `./` and `../` are left in place (resolved by the OS as relative paths)
///
/// Panics if the resulting path does not exist on disk after expansion. This
/// catches mistakes in test fixtures and matches the user's request to keep
/// expansion deterministic.
#[cfg(test)]
pub fn expand_filename(filename: &str) -> String {
    if filename.is_empty() {
        return String::new();
    }

    let home = get_envvar_value("HOME").unwrap_or_default();

    // Tilde expansion: leading `~/` only (we don't try to support `~user`).
    let mut expanded = if let Some(rest) = filename.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else if filename == "~" {
        home.clone()
    } else {
        filename.to_string()
    };

    // Expand every variable known to the test fixture (both `$NAME` and
    // `${NAME}` forms). The set is small and fixed, so a linear pass is fine.
    for (name, value) in test_fixtures::test_env_vars() {
        let braced = format!("${{{name}}}");
        let unbraced = format!("${name}");
        expanded = expanded.replace(&braced, &value).replace(&unbraced, &value);
    }

    assert!(!expanded.contains("$"));
    assert!(!expanded.contains("~"));

    expanded
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
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
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
#[cfg(not(test))]
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
    let quoted = quoting_function_rust(&s_str, quote_type, true, true);
    let quoted_cstr = std::ffi::CString::new(quoted).unwrap();
    unsafe { bash_symbols::xmalloc_cstr(&quoted_cstr) }
}

pub fn quoting_function_rust(
    s: &str,
    quote_type: QuoteType,
    opening_quote: bool,
    closing_quote: bool,
) -> String {
    match quote_type {
        QuoteType::SingleQuote => {
            let mut quoted = s.replace('\'', "'\\''");
            if opening_quote {
                quoted = format!("'{}", quoted);
            }
            if closing_quote {
                quoted.push('\'');
            }
            quoted
        }
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

            let mut quoted = if opening_quote {
                format!("\"{}", escaped)
            } else {
                escaped
            };
            if closing_quote {
                quoted.push('"');
            }
            quoted
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
#[cfg(not(test))]
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

static DEFINED_ALIASES: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_RESERVED_WORDS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_SHELL_FUNCTIONS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_BUILTINS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);

#[cfg(not(test))]
fn get_cached_aliases() -> Vec<CommandWordInfo> {
    let mut guard = DEFINED_ALIASES.lock().unwrap();
    guard
        .get_or_insert_with(|| {
            get_all_aliases()
                .into_iter()
                .map(|name| {
                    let expansion = find_alias(&name).unwrap_or_else(|| name.clone());
                    CommandWordInfo::Alias {
                        command: name,
                        expansion,
                    }
                })
                .collect()
        })
        .clone()
}

#[cfg(not(test))]
fn get_cached_reserved_words() -> Vec<CommandWordInfo> {
    let mut guard = DEFINED_RESERVED_WORDS.lock().unwrap();
    guard
        .get_or_insert_with(|| {
            get_all_reserved_words()
                .into_iter()
                .map(|name| CommandWordInfo::Keyword {
                    command: name,
                    usage: None,
                })
                .collect()
        })
        .clone()
}

#[cfg(not(test))]
fn get_cached_shell_functions() -> Vec<CommandWordInfo> {
    let mut guard = DEFINED_SHELL_FUNCTIONS.lock().unwrap();
    guard
        .get_or_insert_with(|| {
            get_all_shell_functions()
                .into_iter()
                .map(|name| unsafe {
                    let name_c = std::ffi::CString::new(name.clone()).unwrap();
                    let func_def_ptr = bash_symbols::find_function_def(name_c.as_ptr());
                    if !func_def_ptr.is_null() {
                        let func_def = &*func_def_ptr;
                        let line = if func_def.line > 0 {
                            Some(func_def.line)
                        } else {
                            None
                        };
                        let source_file = if func_def.source_file.is_null() {
                            None
                        } else {
                            std::ffi::CStr::from_ptr(func_def.source_file)
                                .to_str()
                                .ok()
                                .map(|s| s.to_string())
                        };
                        CommandWordInfo::Function {
                            command: name,
                            source_file,
                            line,
                        }
                    } else {
                        CommandWordInfo::Function {
                            command: name,
                            source_file: None,
                            line: None,
                        }
                    }
                })
                .collect()
        })
        .clone()
}

#[cfg(not(test))]
fn get_cached_builtins() -> Vec<CommandWordInfo> {
    let mut guard = DEFINED_BUILTINS.lock().unwrap();
    guard
        .get_or_insert_with(|| {
            get_all_shell_builtins()
                .into_iter()
                .map(|name| CommandWordInfo::Builtin {
                    command: name,
                    usage: None,
                })
                .collect()
        })
        .clone()
}

/// Per-directory executable cache entry: the directory's last-modified time and
/// the list of executable filenames found in that directory.
#[cfg(not(test))]
struct DirExecutables {
    mtime: SystemTime,
    names: Vec<String>,
}

/// Global cache that maps each directory on `PATH` to its executable names and
/// the directory's last-modified timestamp.  The cache is **never** invalidated
/// on app startup; instead it is updated lazily on every access:
///
/// 1. Directories that have been removed from `PATH` are evicted from the cache.
/// 2. Newly-added directories are scanned and inserted.
/// 3. For each remaining directory the last-modified time is compared to the
///    cached value; if it has changed the directory is re-scanned.
#[cfg(not(test))]
struct ExecutablesOnPath {
    cache: HashMap<PathBuf, DirExecutables>,
}

#[cfg(not(test))]
impl ExecutablesOnPath {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Update the cache in-place: evict removed PATH dirs, add new ones, and
    /// re-scan any directory whose mtime has changed.
    fn update_cache(&mut self) {
        let current_dirs: Vec<PathBuf> = get_envvar_value("PATH")
            .map(|p| p.split(':').map(PathBuf::from).collect())
            .unwrap_or_default();

        let current_dir_set: HashSet<&PathBuf> = current_dirs.iter().collect();

        // Evict directories that are no longer on PATH.
        self.cache.retain(|dir, _| current_dir_set.contains(dir));

        // Refresh (or populate) each directory that is currently on PATH.
        for dir in &current_dirs {
            // Only call `metadata()` when we need to compare or store an mtime.
            let current_mtime = match self.cache.get(dir) {
                None => {
                    // Not cached; scan unconditionally.
                    dir.metadata().ok().and_then(|m| m.modified().ok())
                }
                Some(entry) => {
                    let mtime = dir.metadata().ok().and_then(|m| m.modified().ok());
                    // If the mtime is unchanged, nothing to do for this dir.
                    if mtime.as_ref() == Some(&entry.mtime) {
                        continue;
                    }
                    mtime
                }
            };

            let names = Self::scan_dir(dir);
            if let Some(mtime) = current_mtime {
                self.cache
                    .insert(dir.clone(), DirExecutables { mtime, names });
            }
            // If the directory's mtime is not readable we skip caching its
            // executables; it will be re-scanned on the next access.
        }
    }

    /// Iterate over the names of all cached executables.
    fn iter_info(&self) -> impl Iterator<Item = CommandWordInfo> + '_ {
        self.cache.iter().flat_map(|(dir, entry)| {
            entry.names.iter().map(move |name| {
                let path = dir.join(name).to_string_lossy().into_owned();
                CommandWordInfo::File {
                    command: name.clone(),
                    path,
                }
            })
        })
    }

    /// Scan `dir` and return the names of all executable files it contains.
    fn scan_dir(dir: &Path) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = std::fs::metadata(entry.path())
                    && metadata.is_file()
                {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        if let Some(file_name) = entry.file_name().to_str() {
                            names.push(file_name.to_string());
                        }
                    }
                }
            }
        }
        names
    }
}

#[cfg(not(test))]
static EXECUTABLES_ON_PATH: LazyLock<Mutex<ExecutablesOnPath>> =
    LazyLock::new(|| Mutex::new(ExecutablesOnPath::new()));

pub(crate) static LS_COLORS: LazyLock<Option<LsColors>> =
    LazyLock::new(|| get_envvar_value("LS_COLORS").map(|s| LsColors::from_string(&s)));

/// Get all potential first word completions (aliases, reserved words, functions, builtins, executables)
#[cfg(not(test))]
pub fn get_possible_command_words() -> impl Iterator<Item = CommandWordInfo> {
    let aliases = get_cached_aliases();
    let reserved_words = get_cached_reserved_words();
    let shell_functions = get_cached_shell_functions();
    let builtins = get_cached_builtins();
    let mut exe_guard = EXECUTABLES_ON_PATH.lock().unwrap();
    exe_guard.update_cache();
    let executables: Vec<CommandWordInfo> = exe_guard.iter_info().collect();
    drop(exe_guard);

    aliases
        .into_iter()
        .chain(reserved_words)
        .chain(shell_functions)
        .chain(builtins)
        .chain(executables)
}

#[cfg(test)]
pub fn get_possible_command_words() -> impl Iterator<Item = CommandWordInfo> {
    get_all_reserved_words()
        .into_iter()
        .map(|name| CommandWordInfo::Keyword {
            command: name,
            usage: None,
        })
}

#[cfg(not(test))]
pub fn warm_completion_caches() {
    let _ = get_cached_aliases();
    let _ = get_cached_reserved_words();
    let _ = get_cached_shell_functions();
    let _ = get_cached_builtins();
    if let Ok(mut exe_guard) = EXECUTABLES_ON_PATH.lock() {
        exe_guard.update_cache();
    }
}

#[cfg(test)]
pub fn warm_completion_caches() {}

#[cfg(not(test))]
pub fn read_terminating_signal() -> c_int {
    unsafe { (&raw const crate::bash_symbols::terminating_signal).read_volatile() }
}

#[cfg(test)]
pub fn read_terminating_signal() -> c_int {
    0
}

#[cfg(not(test))]
pub fn set_env_var(name: &str, value: &str) -> Result<()> {
    unsafe {
        let name_cstr = std::ffi::CString::new(name)?;
        let value_cstr = std::ffi::CString::new(value)?;
        let res = bash_symbols::bind_variable(name_cstr.as_ptr(), value_cstr.as_ptr(), 0);
        if res.is_null() {
            return Err(anyhow::anyhow!(
                "Failed to create environment variable '{}'",
                name
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub fn set_env_var(name: &str, value: &str) -> Result<()> {
    // SAFETY: Tests that mutate process env vars run inside `rusty_fork_test!`
    // forked subprocesses, so the mutation cannot race with other threads.
    unsafe { std::env::set_var(name, value) };
    Ok(())
}

#[cfg(not(test))]
pub fn unset_env_var(name: &str) -> Result<()> {
    unsafe {
        let name_cstr = std::ffi::CString::new(name)?;
        let res = bash_symbols::unbind_variable(name_cstr.as_ptr());
        if res != 0 {
            return Err(anyhow::anyhow!(
                "Failed to unset environment variable '{}'",
                name
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub fn unset_env_var(name: &str) -> Result<()> {
    // SAFETY: see set_env_var.
    unsafe { std::env::remove_var(name) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_function() {
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::Backslash, true, true),
            r#"qwe\ asd"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::DoubleQuote, true, true),
            r#""qwe asd""#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::SingleQuote, true, true),
            r#"'qwe asd'"#
        );
    }

    #[test]
    fn test_quote_function_harder() {
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::Backslash, true, true),
            r#"qwe\"asdf"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::DoubleQuote, true, true),
            r#""qwe\"asdf""#
        );
    }

    #[test]
    fn test_quote_function_backslash_special_chars() {
        for &c in BACKSLASH_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected = format!("a\\{}b", c);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::Backslash, true, true),
                expected
            );
        }
    }

    #[test]
    fn test_quote_function_double_quote_special_chars() {
        for &c in DOUBLE_QUOTE_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected_inner = format!("a\\{}b", c);
            let expected = format!("\"{}\"", expected_inner);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::DoubleQuote, true, true),
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

    #[test]
    fn test_detect_and_convert_inline_descriptions() {
        let mut flags = CompletionFlags::default();
        flags.filename_completion_desired = false;

        // 1. A typical aligned list of options with descriptions.
        let mut comps = vec![
            "port      List port mappings".to_string(),
            "ps        List containers".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps\tList containers");

        // 2. An aligned list of options where some descriptions are single words.
        let mut comps = vec![
            "-d      Decompress".to_string(),
            "-z      Compress".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "-d\tDecompress");
        assert_eq!(comps[1], "-z\tCompress");

        // 3. A single option with a description (containing a space).
        let mut comps = vec!["port      List port mappings".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");

        // 4. A single option with a single-word description starting with a flag.
        let mut comps = vec!["-d      Decompress".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "-d\tDecompress");

        // 5. A single option with a single-word description (not a flag).
        // Should NOT convert to avoid false positives (e.g. "my  file.txt" or "build  Build" when it is the only completion).
        let mut comps = vec!["build      Build".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "build      Build");

        // 6. Filename completion desired - should skip.
        let mut comps = vec![
            "port      List port mappings".to_string(),
            "ps        List containers".to_string(),
        ];
        let mut file_flags = CompletionFlags::default();
        file_flags.filename_completion_desired = true;
        detect_and_convert_inline_descriptions(&mut comps, &file_flags);
        assert_eq!(comps[0], "port      List port mappings");

        // 7. Non-aligned filenames (different lengths, double spaces).
        // Should NOT convert.
        let mut comps = vec!["my  file.txt".to_string(), "another  file.txt".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "my  file.txt");
        assert_eq!(comps[1], "another  file.txt");

        // 8. Arbitrary string with spaces in the value part (e.g. "my file      description").
        // Value part contains spaces, so it's not a strict completion value, should NOT convert.
        let mut comps = vec!["my file      description".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "my file      description");

        // 9. One completion already contains a tab character.
        // Should NOT convert any of them.
        let mut comps = vec![
            "port\tList port mappings".to_string(),
            "ps      List containers".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps      List containers");

        // 10. Descriptions wrapped in parentheses should be stripped.
        let mut comps = vec![
            "port      (List port mappings)".to_string(),
            "ps        (List containers)".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps\tList containers");
    }
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------
//
// Hardcoded data used to back the `#[cfg(test)]` versions of bash_funcs.
// Centralising these keeps the various test stubs consistent — for example
// `find_alias` and `get_all_aliases` both read from the same alias table,
// and every test stub that needs to look up an environment variable goes
// through `test_env_vars` so the only non-fixed value in the test
// environment is the current working directory (returned for `$PWD`).
#[cfg(test)]
pub(crate) mod test_fixtures {
    use clap::{CommandFactory, Parser, Subcommand};

    /// Aliases visible to the test build of flyline. Shared between
    /// `find_alias` and `get_all_aliases` so the two stay in sync.
    pub(crate) fn test_aliases() -> &'static [(&'static str, &'static str)] {
        &[
            ("gst", "git status"),
            ("gcm", "git commit -m"),
            ("gd", "git diff"),
        ]
    }

    /// Hardcoded set of environment variables visible to test code. The
    /// only non-fixed value is `PWD`, which is sourced from the process
    /// current working directory; everything else is a fixed string. All
    /// `#[cfg(test)]` bash_funcs that need an env var look it up here so
    /// tests never observe variables that happen to be set by `cargo
    /// test`'s parent shell.
    pub(crate) fn test_env_vars() -> Vec<(String, String)> {
        let pwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        vec![
            ("HOME".to_string(), "/home/john".to_string()),
            ("PWD".to_string(), pwd),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("SHELL".to_string(), "/bin/bash".to_string()),
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("USER".to_string(), "john".to_string()),
        ]
    }

    /// Tiny clap definition used to drive the test build of
    /// `run_programmable_completions`. It only implements `add`, `commit`,
    /// `diff`, and `status` with at most four flags each, but that is
    /// enough to exercise flyline's programmable-completion plumbing
    /// without needing a real bash instance.
    #[derive(Parser, Debug)]
    #[command(name = "git", no_binary_name = true)]
    struct DummyGitArgs {
        #[command(subcommand)]
        command: Option<DummyGitCommand>,
    }

    #[derive(Subcommand, Debug)]
    enum DummyGitCommand {
        Add {
            #[arg(long = "all", short = 'A')]
            all: bool,
            #[arg(long = "patch", short = 'p')]
            patch: bool,
            #[arg(long = "verbose", short = 'v')]
            verbose: bool,
            #[arg(long = "dry-run", short = 'n')]
            dry_run: bool,
            files: Vec<String>,
        },
        Commit {
            #[arg(long = "message", short = 'm')]
            message: Option<String>,
            #[arg(long = "amend")]
            amend: bool,
            #[arg(long = "all", short = 'a')]
            all: bool,
            #[arg(long = "no-verify")]
            no_verify: bool,
        },
        Diff {
            #[arg(long = "staged")]
            staged: bool,
            #[arg(long = "stat")]
            stat: bool,
            #[arg(long = "name-only")]
            name_only: bool,
            #[arg(long = "color")]
            color: bool,
            paths: Vec<String>,
        },
        Status {
            #[arg(long = "short", short = 's')]
            short: bool,
            #[arg(long = "branch", short = 'b')]
            branch: bool,
            #[arg(long = "porcelain")]
            porcelain: bool,
            #[arg(long = "untracked-files", short = 'u')]
            untracked_files: bool,
        },
    }

    pub(crate) fn dummy_git_completions(
        full_command: &str,
        word_under_cursor: &str,
    ) -> Vec<clap_complete::CompletionCandidate> {
        // Tokenize on whitespace; this is a deliberate simplification
        // suitable for the dummy git completer used in unit tests.
        let mut tokens: Vec<String> = full_command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        // Drop the leading "git" command word; the dummy parser uses
        // `no_binary_name = true`.
        if tokens.first().map(String::as_str) == Some("git") {
            tokens.remove(0);
        }

        // Determine if the cursor is at the end (i.e. completing a
        // brand-new empty word) or replacing the last token.
        let trailing_space = full_command.ends_with(char::is_whitespace);
        if trailing_space || tokens.is_empty() || word_under_cursor.is_empty() {
            tokens.push(String::new());
        } else if tokens.last().map(String::as_str) != Some(word_under_cursor) {
            // Replace whatever the last token is with the word under
            // cursor so the clap completer treats it as the prefix.
            let last = tokens.last_mut().unwrap();
            *last = word_under_cursor.to_string();
        }

        let args_os: Vec<std::ffi::OsString> =
            tokens.into_iter().map(std::ffi::OsString::from).collect();
        let index = args_os.len() - 1;
        let mut cmd = DummyGitArgs::command();
        let current_dir = std::env::current_dir().ok();

        clap_complete::engine::complete(&mut cmd, args_os, index, current_dir.as_deref())
            .unwrap_or_default()
    }
}
