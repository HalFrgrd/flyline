use crate::bash_symbols;

use std::os::raw::c_int;

pub fn call_type(cmd: &str) {
    // Call the `type` builtin to check if the command exists
    let flags = bash_symbols::CDescFlag::All as c_int | bash_symbols::CDescFlag::ShortDesc as c_int;

    let cmd_c_str = std::ffi::CString::new(cmd).unwrap();

    println!("Calling type for command: {} flags={}", cmd, flags);
    let result = unsafe { bash_symbols::describe_command(cmd_c_str.as_ptr(), flags) };

    println!("Result of type {}: {}", cmd, result);
}
