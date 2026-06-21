use std::process::Command;

const BASH_SYMBOLS: &[&str] = &[
    "stream_list",
    "bash_input",
    "push_stream",
    "pop_stream",
    "interactive",
    "interactive_shell",
    "no_line_editing",
    "with_input_from_stdin",
    "get_alias_value",
    "find_function_def",
    "describe_command",
    "programmable_completions",
    "progcomp_search",
    "bash_default_completion",
    "rl_line_buffer",
    "rl_point",
    "rl_end",
    "rl_completion_found_quote",
    "rl_completion_quote_character",
    "rl_filename_quoting_desired",
    "rl_filename_completion_desired",
    "rl_completion_suppress_append",
    "rl_completion_append_character",
    "rl_sort_completion_matches",
    "rl_filename_dequoting_function",
    "rl_filename_quoting_function",
    "pcomp_set_readline_variables",
    "all_aliases",
    "all_variables_matching_prefix",
    "all_shell_functions",
    "shell_builtins",
    "num_shell_builtins",
    "rl_readline_state",
    "current_command_line_count",
    "history_list",
    "current_readline_prompt",
    "getenv",
    "find_variable",
    "bind_variable",
    "unbind_variable",
    "evalstring",
    "parse_and_execute",
    "decode_prompt_string",
    "expand_string_to_string",
    "xmalloc",
    "xrealloc",
    "xfree",
    "terminating_signal",
    "termsig_handler",
    "rl_signal_event_hook",
    "job_control",
    "give_terminal_to",
    "shell_pgrp",
    "last_command_exit_value",
    "get_working_directory",
    "current_host_name",
    "show_var_attributes",
];

fn main() {
    // Capture git commit hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    // Capture build datetime (UTC, ISO 8601) using chrono (already a project dependency)
    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_TIME={build_time}");

    // Re-run when HEAD changes (branch switch or detached-HEAD commit)
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Re-run when the example agent mode file changes (embedded via include_str! in agent_mode.rs)
    println!("cargo:rerun-if-changed=examples/agent_mode.sh");
    // Re-run when the current branch ref changes (new commit on a branch)
    if let Ok(head) = std::fs::read_to_string(".git/HEAD")
        && let Some(refpath) = head.strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=.git/{}", refpath.trim());
    }

    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let def_path = std::path::Path::new(&out_dir).join("bash.def");
        let lib_path = std::path::Path::new(&out_dir).join("libbash.dll.a");

        let mut def_content = String::new();
        def_content.push_str("LIBRARY bash.exe\nEXPORTS\n");
        for sym in BASH_SYMBOLS {
            def_content.push_str(&format!("    {}\n", sym));
        }
        std::fs::write(&def_path, def_content).expect("Failed to write bash.def");

        // Determine which dlltool to run.
        let dlltool_cmds = ["x86_64-w64-mingw32-dlltool", "llvm-dlltool", "dlltool"];
        let mut success = false;
        for cmd in &dlltool_cmds {
            let status = Command::new(cmd)
                .args([
                    "-d",
                    def_path.to_str().unwrap(),
                    "-D",
                    "bash.exe",
                    "-l",
                    lib_path.to_str().unwrap(),
                ])
                .status();
            if let Ok(s) = status
                && s.success()
            {
                success = true;
                break;
            }
        }
        if !success {
            panic!(
                "Could not find or run a working dlltool (tried x86_64-w64-mingw32-dlltool, llvm-dlltool, dlltool)"
            );
        }

        println!("cargo:rustc-link-search=native={}", out_dir);
        println!("cargo:rustc-link-lib=dylib=bash");

        // Compile dummy bash.exe so tests/runners can load
        let dummy_c_path = std::path::Path::new(&out_dir).join("dummy_bash.c");
        let mut dummy_c = String::new();
        for sym in BASH_SYMBOLS {
            dummy_c.push_str(&format!("__declspec(dllexport) void {}() {{}}\n", sym));
        }
        std::fs::write(&dummy_c_path, dummy_c).expect("Failed to write dummy_bash.c");

        let dummy_exe_path = std::path::Path::new(&out_dir).join("bash.exe");
        let gcc_cmds = ["x86_64-w64-mingw32-gcc", "gcc", "clang"];
        let mut compiled = false;
        for gcc_cmd in &gcc_cmds {
            if let Ok(s) = Command::new(gcc_cmd)
                .args([
                    "-shared",
                    "-o",
                    dummy_exe_path.to_str().unwrap(),
                    dummy_c_path.to_str().unwrap(),
                ])
                .status()
            {
                if s.success() {
                    compiled = true;
                    break;
                }
            }
        }

        if compiled {
            if let Some(target_dir) = std::path::Path::new(&out_dir)
                .parent() // build/flyline-hash/
                .and_then(|p| p.parent()) // build/
                .and_then(|p| p.parent())
            // debug/ or release/
            {
                let _ = std::fs::copy(&dummy_exe_path, target_dir.join("bash.exe"));
                let _ = std::fs::copy(&dummy_exe_path, target_dir.join("deps").join("bash.exe"));
            }
        }
    }
}
