use crate::active_suggestions::{ActiveSuggestions, Suggestion};
use crate::app::App;
use crate::bash_funcs;
use crate::tab_completion_context;
use glob::glob;
use std::path::Path;

/// bash programmable completions:
///
/// - bashline.c: initialize_readline:
///    - rl_attempted_completion_function = attempt_shell_completion;
///
/// - complete.c: rl_complete_internal:
///     - sets our_func to rl_completion_entry_function or backup rl_filename_completion_function
///     - gen_completion_matches:
///         - sets rl_completion_found_quote
///         - sets rl_completion_quote_character
///         - calls rl_attempted_completion_function (which is attempt_shell_completion)
///             - bashline.c: attempt_shell_completion:
///                 - this figures out if we are completing the first word, an env var, tilde expansion, or if we should call the programmable completion function for the command.
///                 - If it detects we want first word completion, it tries to find a special compspec: `iw_compspec = progcomp_search (INITIALWORD)`
///                     it calls: `programmable_completions (INITIALWORD = "_InitialWorD_", text, s, e, &foundcs)`. I assume `text` is the first word.
///                 - The core call is to `programmable_completions`
///         - If that doesnt return any completions, it falls back to `our_func`
///     - if rl_completion_found_quote, it think it tries to undo the quote escaping
///     - when inserting the match, I think it tries to do quoting /  escaping based on what the  word_under_cursor looks like and what rl_completion_quote_character is set to.
///        e.g. if you have a folder called `qwe asd` and you type `cd qw` and tab complete, it will insert `cd qwe\ asd/`
///        but if you type `cd "qw` and tab complete, it will insert `cd "qwe asd"/`
///

// Something I have noticed is that `compgen` behaviour depends  on  `rl_completion_found_quote` and  some other  readline global variables.
// For instance, I think `compgen -d` eventually calls `pcomp_filename_completion_function` which has some escaping logic:
//   iscompgen = this_shell_builtin == compgen_builtin;
//   iscompleting = RL_ISSTATE (RL_STATE_COMPLETING);
//   if (iscompgen && iscompleting == 0 && rl_completion_found_quote == 0
//   && rl_filename_dequoting_function) { ... }

// TODO: instead of trying to do my own first word completion, I could  try to leverage bash's solution.
// TODO: figure out escapements / quoting.
// TODO probably need to set
//   rl_filename_quoting_function = bash_quote_filename;
//   rl_filename_dequoting_function = bash_dequote_filename;
//   rl_char_is_quoted_p = char_is_quoted; // TODO  probably not necessary?

impl App {
    pub fn start_tab_complete(&mut self) {
        let buffer: &str = self.buffer.buffer();
        let completion_context =
            tab_completion_context::get_completion_context(buffer, self.buffer.cursor_byte_pos());

        log::debug!("Completion context: {:?}", completion_context);

        let word_under_cursor = completion_context.word_under_cursor;

        match completion_context.comp_type {
            tab_completion_context::CompType::FirstWord => {
                let completions = self.tab_complete_first_word(word_under_cursor);
                log::debug!("First word completions: {:?}", completions);
                self.try_accept_tab_completion(ActiveSuggestions::try_new(
                    completions,
                    word_under_cursor,
                    &self.buffer,
                ));
            }
            tab_completion_context::CompType::CommandComp { mut command_word } => {
                // This isnt just for commands like `git`, `cargo`
                // Because we call bash_symbols::programmable_completions
                // Bash also completes env vars (`echo $HO`) and other useful completions.
                // Bash doesnt handle alias expansion well:
                // https://www.reddit.com/r/bash/comments/eqwitd/programmable_completion_on_expanded_aliases_not/
                // Since aliases are the highest priority in command word resolution,
                // If it is an alias, lets expand it here for better completion results.
                let poss_alias = bash_funcs::find_alias(&command_word);
                log::debug!(
                    "Checking for alias for command word '{}': {:?}",
                    command_word,
                    poss_alias
                );

                let alias = if let Some(a) = poss_alias
                    && !a.is_empty()
                {
                    a
                } else {
                    command_word.clone()
                };

                let len_delta = alias.len() as isize - command_word.len() as isize;
                let word_under_cursor_end = {
                    let word_start_offset_in_context = word_under_cursor.as_ptr() as usize
                        - completion_context.context.as_ptr() as usize;
                    (word_start_offset_in_context + word_under_cursor.len())
                        .saturating_add_signed(len_delta)
                };

                // this it the cursor position relative to the start of the completion context
                let cursor_byte_pos = completion_context
                    .context_until_cursor
                    .len()
                    .saturating_add_signed(len_delta);

                let full_command =
                    alias.to_string() + &completion_context.context[command_word.len()..];
                command_word = alias.split_whitespace().next().unwrap().to_string();

                let poss_completions = bash_funcs::run_autocomplete_compspec(
                    &full_command,
                    &command_word,
                    &word_under_cursor,
                    cursor_byte_pos,
                    word_under_cursor_end,
                );
                match poss_completions {
                    Ok((completions, quote_type)) => {
                        log::debug!("Bash autocomplete results for command: {}", full_command);
                        self.try_accept_tab_completion(ActiveSuggestions::try_new(
                            Suggestion::from_string_vec(completions, "", " ", quote_type),
                            word_under_cursor,
                            &self.buffer,
                        ));
                    }
                    Err(e) => {
                        log::debug!(
                            "Bash autocompletion failed for command: {} with error: {}. Falling back to glob expansion.",
                            full_command,
                            e
                        );
                        let completions = self.tab_complete_current_path(word_under_cursor);
                        self.try_accept_tab_completion(ActiveSuggestions::try_new(
                            completions,
                            word_under_cursor,
                            &self.buffer,
                        ));
                    }
                }
            }
            // tab_completion::CompType::CursorOnBlank(word_under_cursor) => {
            //     log::debug!("Cursor is on blank space, no tab completion performed");
            //     let completions = self.tab_complete_current_path("");
            //     self.active_tab_suggestions = ActiveSuggestions::try_new(
            //         completions
            //             .into_iter()
            //             .map(|mut sug| {
            //                 sug.prefix = " ".to_string();
            //                 sug
            //             })
            //             .collect(),
            //         word_under_cursor,
            //         &mut self.buffer,
            //     );
            // }
            tab_completion_context::CompType::EnvVariable => {
                log::debug!(
                    "Environment variable completion not yet implemented: {:?}",
                    word_under_cursor
                );
            }
            tab_completion_context::CompType::TildeExpansion => {
                log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
                let completions = self.tab_complete_tilde_expansion(&word_under_cursor);
                self.try_accept_tab_completion(ActiveSuggestions::try_new(
                    completions,
                    word_under_cursor,
                    &self.buffer,
                ));
            }
            tab_completion_context::CompType::GlobExpansion => {
                log::debug!("Glob expansion for: {:?}", word_under_cursor);
                let completions = self.tab_complete_glob_expansion(&word_under_cursor);

                // Unlike other completions, if there are multiple glob completions,
                // we join them with spaces and insert them all at once.
                let completions_as_string = completions.iter().map(|sug| sug.s.clone()).fold(
                    String::new(),
                    |mut acc, s| {
                        if !acc.is_empty() {
                            acc.push(' ');
                        }
                        acc.push_str(&s);
                        acc
                    },
                );
                if completions_as_string.is_empty() {
                    log::debug!(
                        "No glob expansion completions found for pattern: {}",
                        word_under_cursor
                    );
                } else {
                    self.try_accept_tab_completion(ActiveSuggestions::try_new(
                        Suggestion::from_string_vec(vec![completions_as_string], "", " ", None),
                        word_under_cursor,
                        &self.buffer,
                    ));
                }
            }
        }
    }

    fn tab_complete_first_word(&self, command: &str) -> Vec<Suggestion> {
        if command.is_empty() {
            return vec![];
        }

        if command.starts_with('.') || command.starts_with('/') {
            // Path to executable
            return self.tab_complete_glob_expansion(&(command.to_string() + "*"));
        }

        let mut res = self.bash_env.get_first_word_completions(&command);

        // TODO: could prioritize based on frequency of use
        res.sort();
        res.sort_by_key(|s| s.len());

        let mut seen = std::collections::HashSet::new();
        res.retain(|s| seen.insert(s.clone()));
        Suggestion::from_string_vec(res, "", " ", None)
    }

    fn tab_complete_current_path(&self, pattern: &str) -> Vec<Suggestion> {
        self.tab_complete_glob_expansion(&(pattern.to_string() + "*"))
    }

    fn expand_path_pattern(&self, pattern: &str) -> (String, Vec<(String, String)>) {
        // TODO expand other variables?
        let mut prefixes_swaps = vec![];
        let mut pattern = pattern.to_string();
        if pattern.starts_with("~/") {
            prefixes_swaps.push((self.home_path.to_string() + "/", "~/".to_string()));
            pattern = pattern.replace(&prefixes_swaps[0].1, &prefixes_swaps[0].0);
        }

        // Resolve the pattern relative to cwd if it's not absolute
        if !Path::new(&pattern).is_absolute() {
            // Get the current working directory for relative paths
            if let Ok(cwd) = std::env::current_dir() {
                if let Some(cwd_str) = cwd.to_str() {
                    prefixes_swaps.push((format!("{}/", cwd_str), "".to_string()));
                    pattern = format!("{}/{}", cwd_str, pattern);
                }
            }
        }

        (pattern, prefixes_swaps)
    }

    fn tab_complete_glob_expansion(&self, pattern: &str) -> Vec<Suggestion> {
        log::debug!("Performing glob expansion for pattern: {}", pattern);
        let (resolved_pattern, prefixes_swaps) = self.expand_path_pattern(pattern);
        log::debug!(
            "resolved_pattern: {} {:?}",
            resolved_pattern,
            prefixes_swaps
        );

        // Use glob to find matching paths
        let mut results = Vec::new();

        const MAX_GLOB_RESULTS: usize = 1_000;

        if let Ok(paths) = glob(&resolved_pattern) {
            for (idx, path_result) in paths.enumerate() {
                if idx >= MAX_GLOB_RESULTS {
                    log::debug!(
                        "Reached maximum glob results limit of {}. Stopping further processing.",
                        MAX_GLOB_RESULTS
                    );
                    break;
                }
                if let Ok(path) = path_result {
                    // Convert the path to a string relative to cwd (or absolute if pattern was absolute)
                    let unexpanded = {
                        let mut p = path.to_string_lossy().to_string();

                        for (prefix_to_remove, prefix_to_replace) in &prefixes_swaps {
                            if p.starts_with(prefix_to_remove) {
                                p = p.replacen(prefix_to_remove, prefix_to_replace, 1);
                            } else {
                                log::warn!(
                                    "Expected path '{}' to start with prefix '{}', but it did not.",
                                    p,
                                    prefix_to_remove
                                );
                                break;
                            }
                        }
                        p
                    };

                    // Add trailing slash for directories
                    if path.is_dir() {
                        // no trailing space for directories
                        results.push(Suggestion::new(
                            format!("{}/", unexpanded),
                            "".to_string(),
                            "".to_string(),
                            None,
                        ));
                    } else {
                        // trailing space for files
                        results.push(Suggestion::new(
                            unexpanded,
                            "".to_string(),
                            " ".to_string(),
                            None,
                        ));
                    }
                }
            }
        }

        results.sort();
        results
    }

    fn tab_complete_tilde_expansion(&self, pattern: &str) -> Vec<Suggestion> {
        let user_pattern = if pattern.starts_with('~') {
            &pattern[1..]
        } else {
            return vec![];
        };

        self.tab_complete_glob_expansion(&("/home/".to_string() + user_pattern + "*"))
    }
}
