use crate::active_suggestions::{ActiveSuggestions, Suggestion};
use crate::app::{App, ContentMode};
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

impl App<'_> {
    fn try_accept_tab_completion(&mut self, opt_suggestion: Option<ActiveSuggestions>) {
        match opt_suggestion.and_then(|s| s.try_accept(&mut self.buffer)) {
            None => {
                self.content_mode = ContentMode::Normal;
            }
            Some(suggestions) => {
                self.content_mode = ContentMode::TabCompletion(suggestions);
            }
        }
    }

    pub fn start_tab_complete(&mut self) {
        let buffer: &str = self.buffer.buffer();
        let completion_context =
            tab_completion_context::get_completion_context(buffer, self.buffer.cursor_byte_pos());

        let suggestions = self.gen_completions_internal(&completion_context);
        match suggestions {
            Some(sugs) => {
                self.try_accept_tab_completion(ActiveSuggestions::try_new(
                    sugs,
                    completion_context.word_under_cursor,
                    &self.buffer,
                ));
            }
            None => {
                log::debug!(
                    "No suggestions generated for completion context: {:?}",
                    completion_context
                );
            }
        }
    }

    fn post_process_completions(
        &self,
        completions: Vec<String>,
        filename_quoting_desired: bool,
        filename_completion_desired: bool,
        no_space_suffix_desired: bool,
        quote_type: Option<bash_funcs::QuoteType>,
    ) -> Vec<Suggestion> {
        completions
        .iter()
        .map(|sug| {
            let quoted = if filename_quoting_desired && filename_completion_desired {
                bash_funcs::quote_function_rust(
                    sug,
                    quote_type.unwrap_or_default(),
                )
            } else {
                sug.clone()
            };

            let space_to_append = if no_space_suffix_desired {
                ""
            } else {
                if sug.ends_with(" ") {
                    ""
                } else {
                    " "
                }
            };

            let (appended, suffix) = if filename_completion_desired {
                let expanded = self.tilde_expand_pattern(sug);
                let path = Path::new(&expanded);
                log::debug!("Checking if path is directory for completion result '{}': {:?} (is_dir: {})", sug, path, path.is_dir());

                if path.is_dir() {
                    (format!("{}/", quoted), "")
                } else {
                    (quoted, space_to_append)
                }
            } else {
                (quoted, space_to_append)
            };

            Suggestion::new(appended, "", suffix)
        })
        .collect::<Vec<_>>()
    }

    pub fn gen_completions_internal(
        &self,
        completion_context: &tab_completion_context::CompletionContext,
    ) -> Option<Vec<Suggestion>> {
        log::debug!("Completion context: {:?}", completion_context);

        let word_under_cursor = completion_context.word_under_cursor;

        match &completion_context.comp_type {
            tab_completion_context::CompType::FirstWord => {
                let completions = self.tab_complete_first_word(word_under_cursor);
                log::debug!("First word completions: {:?}", completions);
                return Some(completions);
            }
            tab_completion_context::CompType::CommandComp {
                command_word: initial_command_word,
            } => {
                // This isnt just for commands like `git`, `cargo`
                // Because we call bash_symbols::programmable_completions
                // Bash also completes env vars (`echo $HO`) and other useful completions.
                // Bash doesnt handle alias expansion well:
                // https://www.reddit.com/r/bash/comments/eqwitd/programmable_completion_on_expanded_aliases_not/
                // Since aliases are the highest priority in command word resolution,
                // If it is an alias, lets expand it here for better completion results.
                let mut command_word = initial_command_word.to_string();
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

                let poss_completions = bash_funcs::run_programmable_completions(
                    &full_command,
                    &command_word,
                    &word_under_cursor,
                    cursor_byte_pos,
                    word_under_cursor_end,
                );

                match poss_completions {
                    Ok(comp_result) if !comp_result.completions.is_empty() => {
                        log::debug!("Bash autocomplete results for command: {}", full_command);
                        log::debug!("Completions: {:?}", comp_result);
    
                        let suggestions = self.post_process_completions(
                            comp_result.completions,
                            comp_result.filename_quoting_desired,
                            comp_result.filename_completion_desired,
                            comp_result.no_suffix_desired,
                            comp_result.quote_type,
                        );
                        return Some(suggestions);
                    }

                    Ok(comp_result) if comp_result.bash_default_fallback_desired => {
                        log::debug!(
                            "Bash autocompletion requested fallback to default completion for command: {}.",
                            full_command
                        );
                        let bash_default_completions = bash_funcs::run_bash_default_completion(
                            &full_command,
                            &word_under_cursor,
                            cursor_byte_pos,
                            word_under_cursor_end,
                            &comp_result,
                        );

                        if let Ok(default_comp_result) = bash_default_completions
                            && !default_comp_result.completions.is_empty()
                        {
                            log::debug!(
                                "Bash default completion results for command: {}",
                                full_command
                            );
                            log::debug!("Completions: {:?}", default_comp_result);

                            let suggestions = self.post_process_completions(
                                default_comp_result.completions,
                                default_comp_result.filename_quoting_desired,
                                default_comp_result.filename_completion_desired,
                                default_comp_result.no_suffix_desired,
                                default_comp_result.quote_type,
                            );
                            return Some(suggestions);
                        }
                    }
                    _ => {
                        log::debug!(
                            "No bash programmable completions found for command: {}. Falling back to default completion.",
                            full_command
                        );
                    }
                }

            }
        }
        match completion_context.comp_type_secondary {
            Some(tab_completion_context::SecondaryCompType::EnvVariable) => {
                log::debug!("Environment variable completion {:?}", word_under_cursor);
                let matching_vars = bash_funcs::get_all_variables_with_prefix(word_under_cursor);
                return Some(Suggestion::from_string_vec(matching_vars, "", " "));
            }
            Some(tab_completion_context::SecondaryCompType::TildeExpansion) => {
                log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
                let completions = self.tab_complete_tilde_expansion(&word_under_cursor);
                return Some(completions);
            }
            Some(tab_completion_context::SecondaryCompType::GlobExpansion) => {
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
                    return Some(Suggestion::from_string_vec(
                        vec![completions_as_string],
                        "",
                        " ",
                    ));
                }
            }
            None => {
                log::debug!(
                    "No secondary completion type detected for: {:?}",
                    word_under_cursor
                );
            }
        }

        None
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

        if res.is_empty() {
            // No prefix matches found, fall back to fuzzy search
            log::debug!("No prefix matches for '{}', trying fuzzy search", command);
            res = self.bash_env.get_fuzzy_first_word_completions(&command);
            return Suggestion::from_string_vec(res, "", " ");
        }

        // TODO: could prioritize based on frequency of use
        res.sort();
        res.sort_by_key(|s| s.len());

        let mut seen = std::collections::HashSet::new();
        res.retain(|s| seen.insert(s.clone()));
        Suggestion::from_string_vec(res, "", " ")
    }

    fn tilde_expand_pattern(&self, pattern: &str) -> String {
        if pattern.starts_with("~/") {
            pattern.replacen("~", &self.home_path, 1)
        } else if pattern.starts_with('~') {
            // This is a naive tilde expansion for other users, it just replaces ~ with /home/ which works in most cases but not all (e.g. if someone has a custom home directory or if it's a different OS). For a more robust solution, we would need to read /etc/passwd or use a crate that can do this for us.
            pattern.replacen("~", "/home/", 1)
        } else {
            pattern.to_string()
        }
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
                        ));
                    } else {
                        // trailing space for files
                        results.push(Suggestion::new(unexpanded, "".to_string(), " ".to_string()));
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

    #[cfg(feature = "integration-tests")]
    pub fn test_tab_completions(&mut self) {
        use crate::logging;
        use itertools::Itertools;

        log::set_max_level(log::LevelFilter::Debug);
        logging::stream_logs("stderr".into()).unwrap();

        let mut run_test_on = |command: &str, expected_suggestions: &[&Suggestion]| {
            self.buffer.replace_buffer(command);
            self.buffer.move_to_end();

            let comp_context = tab_completion_context::get_completion_context(
                self.buffer.buffer(),
                self.buffer.cursor_byte_pos(),
            );
            let some_suggestions = self.gen_completions_internal(&comp_context);

            if some_suggestions.is_none() {
                if expected_suggestions.is_empty() {
                    log::debug!(
                        "No suggestions generated for command '{}', as expected.",
                        command
                    );
                    return;
                } else {
                    panic!(
                        "Expected some tab completion suggestions for command '{}', but got None",
                        command
                    );
                }
            }

            let mut suggestions = some_suggestions.unwrap();

            suggestions.sort_by(|a, b| a.s.cmp(&b.s));

            for sug in &suggestions {
                log::debug!(
                    "Generated suggestion for command '{}': '{:?}'",
                    command,
                    sug
                );
            }

            for pair in suggestions.iter().zip_longest(expected_suggestions.iter()) {
                match pair {
                    itertools::EitherOrBoth::Both(sug, &expected) => {
                        assert_eq!(
                            sug, expected,
                            "For command '{}', expected suggestion '{:?}' but got '{:?}'",
                            command, expected, sug
                        );
                    }
                    itertools::EitherOrBoth::Left(sug) => {
                        panic!(
                            "For command '{}', got unexpected extra suggestion: '{:?}'",
                            command, sug
                        );
                    }
                    itertools::EitherOrBoth::Right(&expected) => {
                        panic!(
                            "For command '{}', expected suggestion '{:?}' was missing",
                            command, expected
                        );
                    }
                }
            }
        };

        run_test_on(
            "flyline_comp_util --filenames ",
            &[
                &Suggestion::new(r#"bar.txt"#, "", " "),
                &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "flyline_comp_util --quoting-desired ",
            &[&Suggestion::new(r#"multi\ word\ option"#, "", " ")],
        );

        run_test_on(
            "flyline_comp_util --suppress-quote ",
            &[&Suggestion::new(r#"multi word option"#, "", " ")],
        );

        run_test_on(
            "flyline_comp_util --dont-suppress-append ",
            &[&Suggestion::new(r#"foo"#, "", " ")],
        );

        run_test_on(
            "flyline_comp_util --suppress-append ",
            &[&Suggestion::new(r#"foo"#, "", "")],
        );

        run_test_on(
            "flyline_comp_util_default_filenames  ",
            &[
                &Suggestion::new(r#"bar.txt"#, "", " "),
                &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        // This fails for some reason
        // run_test_on(
        //     "echo $FOOBARBA",
        //     &[
        //         &Suggestion::new(r#"$FOOBARBAZ"#, "", " "),
        //     ],
        // );

        println!("Tab completion tests FLYLINE_TEST_SUCCESS");
    }
}
