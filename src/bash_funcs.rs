use crate::bash_symbols;

use std::io::Read;
use std::os::raw::c_int;
use std::os::unix::io::FromRawFd;

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

    (result, output.trim().to_string())
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

pub fn call_type(cmd: &str) -> (CommandType, String) {
    // Call the `type` builtin to check if the command exists
    let cmd_c_str = std::ffi::CString::new(cmd).unwrap();

    let (_, command_type_output) = with_redirected_stdout(|| unsafe {
        bash_symbols::describe_command(cmd_c_str.as_ptr(), bash_symbols::CDescFlag::Type as c_int)
    });
    let command_type = CommandType::from_str(&command_type_output);

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
            (result, extracted)
        }
        CommandType::Builtin => {
            let (result, output) = with_redirected_stdout(|| unsafe {
                bash_symbols::describe_command(
                    cmd_c_str.as_ptr(),
                    bash_symbols::CDescFlag::ShortDesc as c_int,
                )
            });

            (result, output)
        }
        CommandType::File => with_redirected_stdout(|| unsafe {
            bash_symbols::describe_command(
                cmd_c_str.as_ptr(),
                bash_symbols::CDescFlag::PathOnly as c_int,
            )
        }),
        _ => {
            // If unknown, no short description
            (0, String::new())
        }
    };

    (command_type, short_desc)
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

    log::debug!("Found aliases: {:?}", aliases);
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

// pub fn get_shell_functions() -> Vec<String> {
//     let mut functions = Vec::new();

//     unsafe {
//         let func_ptr = bash_symbols::all_shell_functions();
//         if func_ptr.is_null() {
//             return functions;
//         }

//         let mut offset = 0;
//         loop {
//             let ptr = *func_ptr.add(offset);
//             if ptr.is_null() {
//                 break;
//             }
//             let c_str = std::ffi::CStr::from_ptr(ptr);
//             if let Ok(str_slice) = c_str.to_str() {
//                 functions.push(str_slice.to_string());
//             }
//             offset += 1;
//         }
//     }

//     log::debug!("Found shell functions: {:?}", functions);
//     functions
// }

pub fn get_all_variables_with_prefix(prefix: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let prefix_c_str = std::ffi::CString::new(prefix).unwrap();

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
                variables.push(str_slice.to_string());
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

    log::debug!("Found shell functions: {:?}", functions);
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

    log::debug!("Found shell builtins: {:?}", builtins);
    builtins
}

pub fn run_autocomplete_compspec(
    full_command: &str,                // "git commi asdf" with cursor just after com
    command_word: &str,                // "git"
    word_under_cursor: &str,           // "commi"
    cursor_byte_pos: usize,            // 7 since cursor is after "com" in "git com|mi asdf"
    word_under_cursor_byte_end: usize, // 9 since we want the end of "commi"
) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    if !full_command.contains(command_word) {
        log::debug!(
            "Command word '{}' not found in full command '{}'",
            command_word,
            full_command
        );
        return res;
    }

    unsafe {
        bash_symbols::pcomp_line = std::ffi::CString::new(full_command).unwrap().into_raw(); // git commi asdf
        bash_symbols::pcomp_ind = cursor_byte_pos as std::ffi::c_int; // 7 ("git com|mi asdf")

        let found: std::ffi::c_int = 0;
        let foundp = &found as *const std::ffi::c_int as *mut std::ffi::c_int;

        let command_word_cstr = std::ffi::CString::new(command_word).unwrap();
        let comp_spec = bash_symbols::progcomp_search(command_word_cstr.as_ptr());
        if !comp_spec.is_null() {
            let compspec_comp = bash_symbols::gen_compspec_completions(
                comp_spec,
                command_word_cstr.as_ptr(),
                std::ffi::CString::new(word_under_cursor).unwrap().as_ptr(),
                0,
                word_under_cursor_byte_end as std::ffi::c_int,
                foundp,
            );
            log::debug!("found value: {}", found);

            if !compspec_comp.is_null() {
                // TODO: verify list len is correct. see the comment in bash_symbols.rs
                log::debug!("compspec_comp result: {:?}", *compspec_comp);
                for i in 0..((*compspec_comp).list_len) {
                    let ptr = *(*compspec_comp).list.add(i as usize);
                    if ptr.is_null() {
                        continue;
                    }
                    let c_str = std::ffi::CStr::from_ptr(ptr);
                    if let Ok(str_slice) = c_str.to_str() {
                        res.push(str_slice.to_string());
                    }
                }
            } else {
                log::debug!("No completions returned from gen_compspec_completions");
            }
        } else {
            log::debug!("No compspec found for command");
        }
    }
    res
}
