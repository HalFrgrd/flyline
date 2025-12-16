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

pub fn get_reserved_words() -> Vec<&'static str> {
    //       { "if", IF },
    //   { "then", THEN },
    //   { "else", ELSE },
    //   { "elif", ELIF },
    //   { "fi", FI },
    //   { "case", CASE },
    //   { "esac", ESAC },
    //   { "for", FOR },
    // #if defined (SELECT_COMMAND)
    //   { "select", SELECT },
    // #endif
    //   { "while", WHILE },
    //   { "until", UNTIL },
    //   { "do", DO },
    //   { "done", DONE },
    //   { "in", IN },
    //   { "function", FUNCTION },
    // #if defined (COMMAND_TIMING)
    //   { "time", TIME },
    // #endif
    //   { "{", '{' },
    //   { "}", '}' },
    //   { "!", BANG },
    // #if defined (COND_COMMAND)
    //   { "[[", COND_START },
    //   { "]]", COND_END },
    // #endif
    // #if defined (COPROCESS_SUPPORT)
    //   { "coproc", COPROC },
    // #endif
    //   { (char *)NULL, 0}

    return vec![
        "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until",
        "do", "done", "in", "function", "time", "{", "}", "!", "[[", "]]", "coproc",
    ];
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

pub fn tab_completion(_buffer: &str) -> Vec<String> {
    // TODO: better first word extraction. see bash source code
    // let first_word = buffer
    //     .split_whitespace()
    //     .next()
    //     .unwrap_or("")
    //     .to_string();

    // let buffer = "gre";
    // unsafe {
    //     bash_symbols::rl_line_buffer = std::ffi::CString::new(buffer).unwrap().into_raw();
    //     bash_symbols::rl_line_buffer_len = buffer.len() as c_int;
    //     let start: c_int = 0;
    //     let end: c_int = buffer.len() as c_int;
    //     let completions_ptr = bash_symbols::attempt_shell_completion(
    //         bash_symbols::rl_line_buffer,
    //         start,
    //         end,
    //     );

    //     if completions_ptr.is_null() {
    //         log::debug!("No completions returned from attempt_shell_completion");
    //     } else {
    //         log::debug!("Completions pointer: {:?}", completions_ptr);
    //         let mut completions = vec![];
    //         let mut offset = 0;
    //         loop {
    //             let ptr = *completions_ptr.add(offset);
    //             if ptr.is_null() {
    //                 break;
    //             }
    //             let c_str = std::ffi::CStr::from_ptr(ptr);
    //             if let Ok(str_slice) = c_str.to_str() {
    //                 completions.push(str_slice.to_string());
    //             }
    //             offset += 1;
    //         }
    //         log::debug!("Completions: {:?}", completions);
    //         return completions;
    //     }

    // }

    // let cmd = std::ffi::CString::new("less").unwrap();
    // unsafe {
    //     let compspec = bash_symbols::progcomp_search(cmd.as_ptr());

    //     if !compspec.is_null() {
    //         log::debug!("Found completion spec for command");
    //         log::debug!("{:?}", *compspec);

    //         // Access fields safely
    //         // if !(*compspec).funcname.is_null() {
    //         //     let funcname = std::ffi::CStr::from_ptr((*compspec).funcname);
    //         //     log::debug!("Function name: {:?}", funcname);
    //     // }
    //     } else {
    //         log::debug!("No completion spec found");
    //     }
    // }

    vec![]
}
