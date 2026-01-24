use crate::bash_funcs;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// Manages bash environment state including caches for command types, aliases, and other bash constructs
pub struct BashEnvManager {
    call_type_cache: HashMap<String, (bash_funcs::CommandType, String)>,
    defined_aliases: Vec<String>,
    defined_reserved_words: Vec<String>,
    defined_shell_functions: Vec<String>,
    defined_builtins: Vec<String>,
    defined_executables: Vec<(PathBuf, String)>,
}

impl BashEnvManager {
    pub fn new() -> Self {
        let path_var = bash_builtins::variables::find_as_string("PATH");
        let executables = if let Some(path_str) = path_var.as_ref().and_then(|v| v.to_str().ok()) {
            Self::get_executables_from_path(path_str)
        } else {
            Vec::new()
        };

        Self {
            call_type_cache: HashMap::new(),
            defined_aliases: bash_funcs::get_all_aliases(),
            defined_reserved_words: bash_funcs::get_all_reserved_words(),
            defined_shell_functions: bash_funcs::get_all_shell_functions(),
            defined_builtins: bash_funcs::get_all_shell_builtins(),
            defined_executables: executables,
        }
    }

    /// Cache and return the command type for a given command
    pub fn cache_command_type(&mut self, cmd: &str) -> (bash_funcs::CommandType, String) {
        if let Some(cached) = self.call_type_cache.get(cmd) {
            return cached.clone();
        }
        let result = bash_funcs::call_type(cmd);
        self.call_type_cache.insert(cmd.to_string(), result.clone());
        // log::debug!("call_type result for {}: {:?}", cmd, result);
        result
    }

    /// Get cached command type without updating cache
    pub fn get_command_info(&self, cmd: &str) -> (bash_funcs::CommandType, String) {
        self.call_type_cache
            .get(cmd)
            .unwrap_or(&(bash_funcs::CommandType::Unknown, String::new()))
            .clone()
    }

    /// Get all potential first word completions (aliases, reserved words, functions, builtins, executables)
    pub fn get_first_word_completions(&self, command: &str) -> Vec<String> {
        let mut res = Vec::new();

        if command.is_empty() {
            return res;
        }

        for poss_completion in self
            .defined_aliases
            .iter()
            .chain(self.defined_reserved_words.iter())
            .chain(self.defined_shell_functions.iter())
            .chain(self.defined_builtins.iter())
            .chain(self.defined_executables.iter().map(|(_, name)| name))
        {
            if poss_completion.starts_with(command) {
                res.push(poss_completion.to_string());
            }
        }

        res
    }

    /// Get executables from PATH environment variable
    fn get_executables_from_path(path_str: &str) -> Vec<(PathBuf, String)> {
        let mut executables = Vec::new();
        for path_dir in path_str.split(':') {
            if let Ok(entries) = std::fs::read_dir(path_dir) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let permissions = metadata.permissions();
                            if permissions.mode() & 0o111 != 0 {
                                // File is executable
                                if let Some(file_name) = entry.file_name().to_str() {
                                    executables.push((entry.path(), file_name.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }
        executables
    }
}
