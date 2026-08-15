use super::symbols as bash_symbols;
use super::symbols::ShellVar;
pub use crate::completions::{CompspecOption, ProgrammableCompleteReturn};
pub use crate::grammar::{
    QuoteType, dequoting_function_rust, find_quote_type, quoting_function_rust,
};
pub use crate::path::EXECUTABLES_ON_PATH;
pub use crate::shell::CommandWordInfo;
use anyhow::Result;

use libc::c_char;
use libc::c_int;
use std::collections::{HashMap, HashSet};

use std::io::Read;

use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::sync::Mutex;

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

pub fn find_alias(cmd: &str) -> Option<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
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

fn get_command_info_uncached(cmd: &str) -> CommandWordInfo {
    let _guard = super::symbols::BASH_LOCK.lock();
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
        _ => {
            if is_autocd_enabled() {
                let expanded = fully_expand_path(cmd);
                // We will only hit the filesystem once per command word
                // would need to rethink this if we weren't caching.
                if !expanded.is_empty() && std::path::Path::new(&expanded).is_dir() {
                    return CommandWordInfo::File {
                        command: cmd.to_string(),
                        path: expanded,
                    };
                }
            }
            CommandWordInfo::Unknown {
                command: cmd.to_string(),
            }
        }
    }
}

static CALL_TYPE_CACHE: Mutex<Option<HashMap<String, CommandWordInfo>>> = Mutex::new(None);

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

pub fn format_shell_var_uncached(name: &str) -> String {
    let _guard = super::symbols::BASH_LOCK.lock();
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

    *DEFINED_ALIASES.lock().unwrap() = None;
    *DEFINED_RESERVED_WORDS.lock().unwrap() = None;
    *DEFINED_SHELL_FUNCTIONS.lock().unwrap() = None;
    *DEFINED_BUILTINS.lock().unwrap() = None;

    crate::git::reset_cache();
}

pub fn get_all_aliases() -> Vec<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
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
        bash_symbols::locked_xfree(alias_ptr as *mut libc::c_void);
    }

    aliases
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

pub fn get_all_variables_with_prefix(prefix: &str) -> Vec<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
    let mut variables = Vec::new();
    let prefix_c_str = std::ffi::CString::new(prefix.strip_prefix('$').unwrap_or(prefix)).unwrap();

    unsafe {
        let var_ptr = bash_symbols::all_variables_matching_prefix(prefix_c_str.as_ptr());
        if var_ptr.is_null() {
            return variables;
        }

        let mut offset = 0;
        let mut ptrs_to_free = Vec::new();
        loop {
            let ptr = *var_ptr.add(offset);
            if ptr.is_null() {
                break;
            }
            let c_str = std::ffi::CStr::from_ptr(ptr);
            if let Ok(str_slice) = c_str.to_str() {
                variables.push(format!("${}", str_slice));
            }
            ptrs_to_free.push(ptr);
            offset += 1;
        }
        for str_ptr in ptrs_to_free {
            bash_symbols::locked_xfree(str_ptr as *mut libc::c_void);
        }
        bash_symbols::locked_xfree(var_ptr as *mut libc::c_void);
    }

    log::debug!("Found variables with prefix '{}': {:?}", prefix, variables);
    variables
}

pub fn get_all_shell_functions() -> Vec<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
    let mut functions = Vec::new();

    unsafe {
        let table_ptr = bash_symbols::shell_functions;
        if table_ptr.is_null() {
            return functions;
        }

        let table = &*table_ptr;
        if table.bucket_array.is_null() || table.nbuckets <= 0 {
            return functions;
        }

        for i in 0..table.nbuckets as isize {
            let mut bucket_ptr = *table.bucket_array.offset(i);
            while !bucket_ptr.is_null() {
                let item = &*bucket_ptr;
                if !item.key.is_null() {
                    if let Ok(name) = std::ffi::CStr::from_ptr(item.key).to_str() {
                        functions.push(name.to_string());
                    }
                }
                bucket_ptr = item.next;
            }
        }
    }

    functions
}

pub fn get_all_shell_builtins() -> Vec<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
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

fn vec_of_strings_from_char_char_ptr(ptr: *mut *mut c_char) -> Vec<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
    let mut strings = Vec::new();
    let mut seen = HashSet::new();
    unsafe {
        if ptr.is_null() {
            return strings;
        }

        // The char** array and its string elements are allocated via xmalloc in Bash's
        // gen_progcomp_completions / gen_action_completions (see mirror-bash/pcomplete.c:1667).
        // Since we invoke programmable_completions out-of-band directly, we must free
        // both the individual strings and the array container using locked_xfree.
        let mut i = 0;
        let mut ptrs_to_free = Vec::new();
        loop {
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
            ptrs_to_free.push(c_str_ptr);
            i += 1;
        }
        for c_str_ptr in ptrs_to_free {
            bash_symbols::locked_xfree(c_str_ptr as *mut libc::c_void);
        }
        bash_symbols::locked_xfree(ptr as *mut libc::c_void);
    }
    strings
}

pub fn useful_compspec_ran(command_word: &str) -> bool {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        let command_word_cstr = match std::ffi::CString::new(command_word) {
            Ok(cstr) => cstr,
            Err(_) => return false,
        };
        let mut compspec_ptr = bash_symbols::progcomp_search(command_word_cstr.as_ptr());
        if compspec_ptr.is_null() {
            // Basename fallback search (matches bash's own logic in pcomplete.c)
            if let Some(pos) = command_word.rfind('/') {
                let basename = &command_word[pos + 1..];
                if !basename.is_empty() {
                    if let Ok(basename_cstr) = std::ffi::CString::new(basename) {
                        compspec_ptr = bash_symbols::progcomp_search(basename_cstr.as_ptr());
                    }
                }
            }
        }
        if compspec_ptr.is_null() {
            log::debug!(
                "useful_compspec_ran: no registered compspec found for '{}' (default/fallback)",
                command_word
            );
            return false;
        }
        let compspec = &*compspec_ptr;
        if compspec.funcname.is_null() {
            if !compspec.command.is_null() {
                if let Ok(cmd_str) = std::ffi::CStr::from_ptr(compspec.command).to_str() {
                    log::debug!(
                        "useful_compspec_ran: registered compspec command for '{}' is: {}",
                        command_word,
                        cmd_str
                    );
                }
            } else {
                log::debug!(
                    "useful_compspec_ran: registered compspec for '{}' has no funcname",
                    command_word
                );
            }
            return true;
        }
        let funcname_cstr = std::ffi::CStr::from_ptr(compspec.funcname);
        if let Ok(funcname_str) = funcname_cstr.to_str() {
            log::debug!(
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

pub fn evaluate_shell_string(script: &str) -> Result<()> {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        let script_cstr = std::ffi::CString::new(script)?;
        let allocated_ptr = bash_symbols::locked_xmalloc_cstr(&script_cstr);
        let from_file_cstr = std::ffi::CString::new("flyline")?;

        #[cfg(not(feature = "pre_bash_4_4"))]
        let flags = bash_symbols::SEVAL_NOHIST
            | bash_symbols::SEVAL_NOOPTIMIZE
            | bash_symbols::SEVAL_NOTIFY;
        #[cfg(feature = "pre_bash_4_4")]
        let flags = bash_symbols::SEVAL_NOHIST | bash_symbols::SEVAL_NOTIFY;

        // Save parser state (Bash's save_parser_state(NULL) uses xmalloc to allocate exact sizeof(sh_parser_state_t))
        let ps_ptr = bash_symbols::save_parser_state(std::ptr::null_mut());

        #[cfg(not(feature = "pre_bash_4_4"))]
        bash_symbols::evalstring(allocated_ptr, from_file_cstr.as_ptr(), flags);
        #[cfg(feature = "pre_bash_4_4")]
        bash_symbols::parse_and_execute(allocated_ptr, from_file_cstr.as_ptr(), flags);

        // Restore parser state so expand_aliases and parser_state are preserved
        if !ps_ptr.is_null() {
            bash_symbols::restore_parser_state(ps_ptr);
            libc::free(ps_ptr);
        }
        Ok(())
    }
}

extern "C" fn quoting_function_c(
    s: *const c_char,
    _rtype: c_int,
    quote_char: *const c_char,
) -> *mut c_char {
    let _guard = super::symbols::BASH_LOCK.lock();
    let s_str = unsafe { std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned() };
    let quote_char_str = unsafe { std::ffi::CStr::from_ptr(quote_char).to_string_lossy() };
    let quote_type = quote_char_str
        .chars()
        .next()
        .and_then(QuoteType::from_char)
        .unwrap_or_default();
    let quoted = quoting_function_rust(&s_str, quote_type, true, true);
    let quoted_cstr = std::ffi::CString::new(quoted).unwrap();
    unsafe { bash_symbols::locked_xmalloc_cstr(&quoted_cstr) }
}

extern "C" fn dequoting_function_c(s: *const c_char, _quote_char: c_int) -> *mut c_char {
    let _guard = super::symbols::BASH_LOCK.lock();
    let s_str = unsafe { std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned() };
    let dequoted = dequoting_function_rust(&s_str);
    let dequoted_cstr = std::ffi::CString::new(dequoted).unwrap();
    unsafe { bash_symbols::locked_xmalloc_cstr(&dequoted_cstr) }
}

pub fn run_programmable_completions(
    full_command: &str,                // "git commi asdf" with cursor just after com
    command_word: &str,                // "git"
    word_under_cursor: &str,           // "commi"
    cursor_byte_pos: usize,            // 7 since cursor is after "com" in "git com|mi asdf"
    word_under_cursor_byte_end: usize, // 9 since we want the end of "commi"
) -> Result<ProgrammableCompleteReturn> {
    let _guard = super::symbols::BASH_LOCK.lock();
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
        bash_symbols::rl_line_buffer = bash_symbols::locked_xmalloc_cstr(&full_command_cstr); // git commi asdf
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
        log::debug!(
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

pub fn get_shell_var(var_name: &str) -> Option<ShellVar> {
    let _guard = super::symbols::BASH_LOCK.lock();
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
    let _guard = super::symbols::BASH_LOCK.lock();
    get_shell_var(var_name).and_then(|var| var.get_value())
}

pub fn get_last_command_exit_value() -> i32 {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe { bash_symbols::last_command_exit_value as i32 }
}

pub fn get_pipestatus() -> Option<String> {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        if let Ok(var_name_cstr) = std::ffi::CString::new("PIPESTATUS") {
            let var_ptr = super::symbols::find_variable(var_name_cstr.as_ptr());
            if !var_ptr.is_null() {
                let var = &*var_ptr;
                if var.is_array() {
                    let elements = var.get_array_elements();
                    if !elements.is_empty() {
                        return Some(elements.join("|"));
                    }
                } else if let Some(val) = var.get_value() {
                    if !val.trim().is_empty() {
                        return Some(val);
                    }
                }
            }
        }
        let last_exit = super::symbols::last_command_exit_value;
        Some(last_exit.to_string())
    }
}

pub fn check_add_history(cmd: &str) -> bool {
    let _guard = super::symbols::BASH_LOCK.lock();
    if let Ok(c_cmd) = std::ffi::CString::new(cmd) {
        unsafe {
            return super::symbols::check_add_history(c_cmd.as_ptr(), 0) != 0;
        }
    }
    true
}

pub fn get_hostname() -> String {
    let _guard = super::symbols::BASH_LOCK.lock();
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
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        // get_working_directory returns a newly allocated string via savestring (using xmalloc)
        // (see mirror-bash/builtins/common.c:618). We must free it with locked_xfree.
        let ptr = bash_symbols::get_working_directory(c"flyline".as_ptr());
        if ptr.is_null() {
            String::new()
        } else {
            let res = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
            bash_symbols::locked_xfree(ptr as *mut libc::c_void);
            res
        }
    }
}

pub fn expand_filename(filename: &str) -> String {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        // expand_string_to_string returns an allocated string via string_list (using xmalloc)
        // (see mirror-bash/subst.c:3859 / 3869). We must free it with locked_xfree.
        let expanded_string = bash_symbols::expand_string_to_string(
            std::ffi::CString::new(filename).unwrap().as_ptr(),
            0,
        );

        if expanded_string.is_null() {
            return filename.to_string();
        }

        let c_str = std::ffi::CStr::from_ptr(expanded_string);
        let res = c_str
            .to_str()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_else(|| filename.to_string());

        bash_symbols::locked_xfree(expanded_string as *mut libc::c_void);
        res
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

pub fn resolve_completion_script_path(
    command_word: &str,
    flycomp_output: Option<&str>,
) -> std::path::PathBuf {
    // Resolve the alias-expanded target command name
    let poss_alias = find_alias(command_word);
    let alias_def = poss_alias
        .as_deref()
        .filter(|alias| !alias.is_empty())
        .unwrap_or(command_word);
    let cmd_word = alias_def
        .split_whitespace()
        .next()
        .unwrap_or(alias_def)
        .to_string();

    // Get the base command filename
    let file_name = std::path::Path::new(&cmd_word)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(&cmd_word);

    // Resolve output completions directory
    let output_dir = flycomp_output.unwrap_or("~/.local/share/bash-completion/completions/");
    let expanded_dir = fully_expand_path(output_dir);

    std::path::Path::new(&expanded_dir).join(file_name)
}

pub fn resolve_and_write_completion_script(
    command_word: &str,
    script: &str,
    flycomp_output: Option<&str>,
) -> Result<std::path::PathBuf, std::io::Error> {
    let write_path = resolve_completion_script_path(command_word, flycomp_output);
    if let Some(parent) = write_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if write_path.exists() {
        let now = chrono::Local::now();
        let datetime_str = now.format("%Y%m%d_%H%M%S").to_string();
        let file_name = write_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(command_word);
        let backup_name = format!("{}_backup_{}", file_name, datetime_str);
        if let Some(parent) = write_path.parent() {
            let backup_path = parent.join(backup_name);
            std::fs::rename(&write_path, &backup_path)?;
        }
    }

    std::fs::write(&write_path, script)?;
    Ok(write_path)
}

// ---------------------------------------------------------------------------
// Cached environment lookups
// ---------------------------------------------------------------------------

static DEFINED_ALIASES: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_RESERVED_WORDS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_SHELL_FUNCTIONS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);
static DEFINED_BUILTINS: Mutex<Option<Vec<CommandWordInfo>>> = Mutex::new(None);

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

fn get_cached_shell_functions() -> Vec<CommandWordInfo> {
    let _guard = super::symbols::BASH_LOCK.lock();
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

/// Get all potential first word completions (aliases, reserved words, functions, builtins, executables)

pub fn get_possible_command_words() -> impl Iterator<Item = CommandWordInfo> {
    let aliases = get_cached_aliases();
    let reserved_words = get_cached_reserved_words();
    let shell_functions = get_cached_shell_functions();
    let builtins = get_cached_builtins();
    // This should be pre warmed by warm_completion_caches
    // We don't update the executables cache here to avoid hitting the filesystem
    // when we are just tab completing
    let executables: Vec<CommandWordInfo> = EXECUTABLES_ON_PATH
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter_info()
        .collect();

    aliases
        .into_iter()
        .chain(reserved_words)
        .chain(shell_functions)
        .chain(builtins)
        .chain(executables)
}

pub fn warm_bash_caches() {
    let _guard = super::symbols::BASH_LOCK.lock();
    let _ = get_cached_aliases();
    let _ = get_cached_reserved_words();
    let _ = get_cached_shell_functions();
    let _ = get_cached_builtins();
}

pub fn read_terminating_signal() -> c_int {
    unsafe { (&raw const super::symbols::terminating_signal).read_volatile() }
}

#[allow(dead_code)]
pub fn set_env_var(name: &str, value: &str) -> Result<()> {
    let _guard = super::symbols::BASH_LOCK.lock();
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

pub fn export_env_var(name: &str, value: &str) -> Result<()> {
    let _guard = super::symbols::BASH_LOCK.lock();
    unsafe {
        let name_cstr = std::ffi::CString::new(name)?;
        let value_cstr = std::ffi::CString::new(value)?;
        let var = bash_symbols::bind_variable(name_cstr.as_ptr(), value_cstr.as_ptr(), 0);
        if var.is_null() {
            return Err(anyhow::anyhow!(
                "Failed to export environment variable '{}'",
                name
            ));
        }
        (*var).attributes |= 0x0000001; // att_exported
        bash_symbols::array_needs_making = 1;
        Ok(())
    }
}

pub fn unset_env_var(name: &str) -> Result<()> {
    let _guard = super::symbols::BASH_LOCK.lock();
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

pub fn is_autocd_enabled() -> bool {
    #[cfg(all(not(test), feature = "pre_bash_4_4"))]
    {
        false
    }
    #[cfg(all(not(test), not(feature = "pre_bash_4_4")))]
    {
        let _guard = super::symbols::BASH_LOCK.lock();
        unsafe { bash_symbols::autocd != 0 }
    }
}
