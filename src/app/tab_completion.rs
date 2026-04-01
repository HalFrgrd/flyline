use crate::active_suggestions::{ActiveSuggestions, Suggestion, UnprocessedSuggestion};
use crate::app::{App, ContentMode};
use crate::bash_funcs::{self, QuoteType};
use crate::tab_completion_context;
use glob::glob;
use std::path::Path;

#[derive(Debug)]
struct PathPatternExpansion {
    /// The part of the pattern before the last `/`, kept in its original form
    /// (e.g. `~/foo` or `relative/dir`).
    raw_prefix: String,
    /// `raw_prefix` after tilde expansion, conversion to an absolute path, and
    /// environment-variable expansion (e.g. `/home/user/foo` or `/cwd/relative/dir`).
    expanded_prefix: String,
    /// The part of the pattern after the last `/` — the glob portion
    /// (e.g. `ba*` or `*.txt`).
    rhs_pattern: String,
}

impl PathPatternExpansion {
    fn new(pattern: &str) -> Self {
        // Find the first unescaped glob metacharacter (* ? [).
        let first_glob_pos = pattern
            .char_indices()
            .find(|&(i, c)| {
                (c == '*' || c == '?' || c == '[') && (i == 0 || pattern.as_bytes()[i - 1] != b'\\')
            })
            .map(|(i, _)| i);

        // When the pattern contains glob characters, split at the last `/`
        // that comes *before* the first glob metacharacter so that the
        // prefix never contains unresolved globs (which would prevent
        // `strip_prefix` from working later).  When there are no glob
        // characters, fall back to splitting at the last `/`.
        let search_end = first_glob_pos.unwrap_or(pattern.len());
        let (raw_prefix, rhs_pattern) = if let Some(slash_pos) = pattern[..search_end].rfind('/') {
            (
                pattern[..slash_pos].to_string(),
                pattern[slash_pos + 1..].to_string(),
            )
        } else {
            (String::new(), pattern.to_string())
        };
        let fully_expanded_prefix = bash_funcs::fully_expand_path(&raw_prefix);

        let rhs_pattern = bash_funcs::dequoting_function_rust(&rhs_pattern);

        PathPatternExpansion {
            raw_prefix,
            expanded_prefix: fully_expanded_prefix,
            rhs_pattern,
        }
    }

    fn expanded_pattern(&self) -> String {
        if self.expanded_prefix.is_empty() {
            self.rhs_pattern.clone()
        } else {
            format!("{}/{}", self.expanded_prefix, self.rhs_pattern)
        }
    }

    fn convert_expanded_match_to_unexpanded(
        &self,
        expanded_match: &str,
        quote_type: Option<QuoteType>,
    ) -> String {
        // Compute the relative path of the result compared to
        // expanded_prefix, then reconstruct using raw_prefix so the
        // suggestion preserves the user's original prefix spelling
        // (e.g. `~/`, `$HOME/`, or a relative path segment).
        if let Some(suffix) = expanded_match.strip_prefix(&self.expanded_prefix) {
            let suffix = suffix.trim_start_matches('/');

            let quoted_suffix = match quote_type {
                Some(QuoteType::DoubleQuote | QuoteType::SingleQuote) => suffix.to_string(),
                _ => bash_funcs::quote_function_rust(suffix, quote_type.unwrap_or_default()),
            };
            if self.raw_prefix.is_empty() {
                quoted_suffix
            } else {
                format!("{}/{}", self.raw_prefix, quoted_suffix)
            }
        } else {
            log::warn!(
                "Expected expanded match '{}' to start with expanded_prefix '{}', but it did not.",
                expanded_match,
                self.expanded_prefix
            );
            expanded_match.to_string()
        }
    }
}

// bash programmable completions:
//
// - bashline.c: initialize_readline:
//    - rl_attempted_completion_function = attempt_shell_completion;
//
// - complete.c: rl_complete_internal:
//     - sets our_func to rl_completion_entry_function or backup rl_filename_completion_function
//     - gen_completion_matches:
//         - sets rl_completion_found_quote
//         - sets rl_completion_quote_character
//         - calls rl_attempted_completion_function (which is attempt_shell_completion)
//             - bashline.c: attempt_shell_completion:
//                 - this figures out if we are completing the first word, an env var, tilde expansion, or if we should call the programmable completion function for the command.
//                 - If it detects we want first word completion, it tries to find a special compspec: `iw_compspec = progcomp_search (INITIALWORD)`
//                     it calls: `programmable_completions (INITIALWORD = "_InitialWorD_", text, s, e, &foundcs)`. I assume `text` is the first word.
//                 - The core call is to `programmable_completions`
//         - If that doesnt return any completions, it falls back to `our_func`
//     - if rl_completion_found_quote, it think it tries to undo the quote escaping
//     - when inserting the match, I think it tries to do quoting /  escaping based on what the  word_under_cursor looks like and what rl_completion_quote_character is set to.
//        e.g. if you have a folder called `qwe asd` and you type `cd qw` and tab complete, it will insert `cd qwe\ asd/`
//        but if you type `cd "qw` and tab complete, it will insert `cd "qwe asd"/`
//

// Something I have noticed is that `compgen` behaviour depends  on  `rl_completion_found_quote` and  some other  readline global variables.
// For instance, I think `compgen -d` eventually calls `pcomp_filename_completion_function` which has some escaping logic:
//   iscompgen = this_shell_builtin == compgen_builtin;
//   iscompleting = RL_ISSTATE (RL_STATE_COMPLETING);
//   if (iscompgen && iscompleting == 0 && rl_completion_found_quote == 0
//   && rl_filename_dequoting_function) { ... }

struct AliasExpandedCompletion {
    command_word: String,
    full_command: String,
    cursor_byte_pos: usize,
    word_under_cursor_end: usize,
}

/// Expands `command_word` through bash alias resolution and recomputes the
/// context offsets to account for any length change introduced by the alias.
///
/// Taking `command_word` by value (ownership) ensures that the pre-expansion
/// name is no longer accessible at the call site after this function is called,
/// preventing accidental re-use of stale data.
///
/// `word_under_cursor` must be a sub-slice of `context`.
fn expand_alias_for_completion(
    command_word: String,
    word_under_cursor: &str,
    context: &str,
    context_until_cursor: &str,
) -> AliasExpandedCompletion {
    let poss_alias = bash_funcs::find_alias(&command_word);
    log::debug!(
        "Checking for alias for command word '{}': {:?}",
        command_word,
        poss_alias
    );

    // Capture the original length before potentially moving `command_word`.
    let command_word_len = command_word.len();

    let alias = if let Some(a) = poss_alias
        && !a.is_empty()
    {
        a
    } else {
        command_word
    };

    let len_delta = alias.len() as isize - command_word_len as isize;
    let word_under_cursor_end = {
        // Safety: `word_under_cursor` is guaranteed by the caller to be a
        // sub-slice of `context`, so this pointer subtraction is valid.
        let word_start_offset_in_context =
            word_under_cursor.as_ptr() as usize - context.as_ptr() as usize;
        (word_start_offset_in_context + word_under_cursor.len()).saturating_add_signed(len_delta)
    };

    // cursor position relative to the start of the completion context
    let cursor_byte_pos = context_until_cursor.len().saturating_add_signed(len_delta);

    let full_command = alias.to_string() + &context[command_word_len..];
    // `alias` is guaranteed non-empty: it is either a non-empty alias string
    // (guarded by `!a.is_empty()` above) or the original non-empty command word.
    let command_word = alias
        .split_whitespace()
        .next()
        .unwrap_or(&alias)
        .to_string();

    AliasExpandedCompletion {
        command_word,
        full_command,
        cursor_byte_pos,
        word_under_cursor_end,
    }
}

impl App<'_> {
    fn try_accept_tab_completion(&mut self, opt_suggestion: Option<ActiveSuggestions>) {
        match opt_suggestion.and_then(|s| s.try_accept(&mut self.buffer)) {
            None => {
                self.content_mode = ContentMode::Normal;
            }
            Some(suggestions) => {
                self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
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
        completions: Vec<String>,
        comp_resultflags: bash_funcs::CompletionFlags,
        word_under_cursor: &str,
    ) -> Vec<UnprocessedSuggestion> {
        completions
            .into_iter()
            .map(|sug| UnprocessedSuggestion::Raw {
                raw_text: sug,
                expanded_path: None,
                flags: comp_resultflags,
                word_under_cursor: word_under_cursor.to_string(),
            })
            .collect()
    }

    pub fn gen_completions_internal(
        &self,
        completion_context: &tab_completion_context::CompletionContext,
    ) -> Option<Vec<UnprocessedSuggestion>> {
        log::debug!("Completion context: {:#?}", completion_context);

        let word_under_cursor = completion_context.word_under_cursor;

        match &completion_context.comp_type {
            tab_completion_context::CompType::FirstWord => {
                let completions = self.tab_complete_first_word(word_under_cursor);
                log::debug!("Primary completions for first word: {:?}", completions);
                if !completions.is_empty() {
                    return Some(completions);
                }
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
                let AliasExpandedCompletion {
                    command_word,
                    full_command,
                    cursor_byte_pos,
                    word_under_cursor_end,
                } = expand_alias_for_completion(
                    initial_command_word.to_string(),
                    word_under_cursor,
                    completion_context.context,
                    completion_context.context_until_cursor,
                );

                let poss_completions = bash_funcs::run_programmable_completions(
                    &full_command,
                    &command_word,
                    word_under_cursor,
                    cursor_byte_pos,
                    word_under_cursor_end,
                );

                match poss_completions {
                    Ok(comp_result) if !comp_result.completions.is_empty() => {
                        log::debug!(
                            "Programmable completion results for command: {}",
                            full_command
                        );
                        log::debug!("Completions: {:#?}", comp_result);

                        let suggestions = Self::post_process_completions(
                            comp_result.completions,
                            comp_result.flags,
                            word_under_cursor,
                        );
                        return Some(suggestions);
                    }
                    Ok(comp_result) => {
                        // I am not checking if the user wants more completions (i.e. readline_default_fallback_desired)
                        // Always try to produce secondary completions
                        return self
                            .gen_secondary_completions(completion_context, comp_result.flags);
                    }
                    _ => {}
                }
            }
        }

        log::debug!(
            "No programmable completions found completion_context: {:#?}. Falling back to secondary completions.",
            completion_context
        );

        self.gen_secondary_completions(completion_context, bash_funcs::CompletionFlags::default())
    }

    fn gen_secondary_completions(
        &self,
        completion_context: &tab_completion_context::CompletionContext,
        comp_resultflags: bash_funcs::CompletionFlags,
    ) -> Option<Vec<UnprocessedSuggestion>> {
        let word_under_cursor = completion_context.word_under_cursor;
        match completion_context.comp_type_secondary {
            Some(tab_completion_context::SecondaryCompType::EnvVariable) => {
                log::debug!("Environment variable completion {:?}", word_under_cursor);
                let matching_vars = bash_funcs::get_all_variables_with_prefix(word_under_cursor);
                return Some(
                    Suggestion::from_string_vec(matching_vars, "", " ")
                        .into_iter()
                        .map(UnprocessedSuggestion::Ready)
                        .collect(),
                );
            }
            Some(tab_completion_context::SecondaryCompType::TildeExpansion) => {
                log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
                let completions = self.tab_complete_tilde_expansion(word_under_cursor);
                return Some(completions);
            }
            Some(tab_completion_context::SecondaryCompType::GlobExpansion) => {
                log::debug!("Glob expansion for: {:?}", word_under_cursor);
                let completions =
                    self.tab_complete_glob_expansion(word_under_cursor, comp_resultflags);

                // Unlike other completions, if there are multiple glob completions,
                // we join them with spaces and insert them all at once.
                // Process each item eagerly here since we need the final text.
                let completions_as_string = completions
                    .iter()
                    .map(|item| item.to_suggestion().s)
                    .fold(String::new(), |mut acc, s| {
                        if !acc.is_empty() {
                            acc.push(' ');
                        }
                        acc.push_str(&s);
                        acc
                    });
                if completions_as_string.is_empty() {
                    log::debug!(
                        "No glob expansion completions found for pattern: {}",
                        word_under_cursor
                    );
                } else {
                    // If the last completion is a directory (ends with '/'), don't
                    // append a trailing space so the cursor stays right after the slash.
                    let suffix = if completions_as_string.ends_with('/') {
                        ""
                    } else {
                        " "
                    };
                    return Some(
                        Suggestion::from_string_vec(vec![completions_as_string], "", suffix)
                            .into_iter()
                            .map(UnprocessedSuggestion::Ready)
                            .collect(),
                    );
                }
            }
            Some(tab_completion_context::SecondaryCompType::FilenameExpansion) => {
                log::debug!("Filename expansion for: {:?}", word_under_cursor);
                let completions = self.tab_complete_glob_expansion(
                    &(word_under_cursor.to_string() + "*"),
                    comp_resultflags,
                );

                if completions.is_empty() {
                    log::debug!(
                        "No filename expansion completions found for pattern: {}",
                        word_under_cursor
                    );
                } else {
                    return Some(completions);
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

    fn tab_complete_first_word(&self, command: &str) -> Vec<UnprocessedSuggestion> {
        log::debug!("Generating first word completions for: '{}'", command);
        if command.is_empty() {
            return vec![];
        }

        if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
            // Path to executable
            return self.tab_complete_glob_expansion(
                &(command.to_string() + "*"),
                bash_funcs::CompletionFlags::default(),
            );
        }

        let mut res = bash_funcs::get_first_word_completions(command);

        if res.is_empty() {
            // No prefix matches found, fall back to fuzzy search
            log::debug!("No prefix matches for '{}', trying fuzzy search", command);
            res = bash_funcs::get_fuzzy_first_word_completions(command);
            return Suggestion::from_string_vec(res, "", " ")
                .into_iter()
                .map(UnprocessedSuggestion::Ready)
                .collect();
        }

        // TODO: could prioritize based on frequency of use
        res.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        res.dedup();
        Suggestion::from_string_vec(res, "", " ")
            .into_iter()
            .map(UnprocessedSuggestion::Ready)
            .collect()
    }

    fn tab_complete_glob_expansion(
        &self,
        pattern: &str,
        mut comp_resultflags: bash_funcs::CompletionFlags,
    ) -> Vec<UnprocessedSuggestion> {
        // We will handle it ourselves because the prefix should not be quoted but the found filename should be.
        // e.g. my_command $PWD/fi<TAB> should expand to:
        // my_command $PWD/file\ with\ spaces.txt
        // not
        // my_command \$PWD/file\ with\ spaces.txt
        comp_resultflags.filename_quoting_desired = false;
        comp_resultflags.filename_completion_desired = true;

        comp_resultflags.quote_type = bash_funcs::find_quote_type(pattern);
        log::debug!("found quote type: {:?}", comp_resultflags.quote_type);

        let expanded = PathPatternExpansion::new(pattern);
        log::debug!("Performing glob expansion for expanded: {:#?}", expanded);

        // Use glob to find matching paths
        let mut results = Vec::new();

        const MAX_GLOB_RESULTS: usize = 1_000;

        if let Ok(paths) = glob(&expanded.expanded_pattern()) {
            for (idx, path_result) in paths.enumerate() {
                if idx >= MAX_GLOB_RESULTS {
                    log::debug!(
                        "Reached maximum glob results limit of {}. Stopping further processing.",
                        MAX_GLOB_RESULTS
                    );
                    break;
                }
                if let Ok(path) = path_result {
                    let unexpanded = expanded.convert_expanded_match_to_unexpanded(
                        &path.to_string_lossy(),
                        comp_resultflags.quote_type,
                    );

                    results.push(UnprocessedSuggestion::Raw {
                        raw_text: unexpanded,
                        expanded_path: Some(path),
                        flags: comp_resultflags,
                        // The glob expansion path already preserves the raw prefix in
                        // `unexpanded` via PathPatternExpansion; pass "" here so
                        // post_process_completion doesn't attempt a second
                        // prefix split (filename_quoting_desired is false anyway).
                        word_under_cursor: String::new(),
                    });
                }
            }
        }

        results.sort_by(|a, b| a.match_text().cmp(b.match_text()));
        results
    }

    fn tab_complete_tilde_expansion(&self, pattern: &str) -> Vec<UnprocessedSuggestion> {
        let user_pattern = if let Some(stripped) = pattern.strip_prefix('~') {
            stripped
        } else {
            return vec![];
        };

        // `~` alone — suggest the current user's home directory as `~/`
        if user_pattern.is_empty() {
            return vec![UnprocessedSuggestion::Ready(Suggestion::new("~/", "", ""))];
        }

        // `~username` — find matching users by listing /home/ and checking /root
        let mut suggestions = Vec::new();

        if let Ok(entries) = std::fs::read_dir("/home") {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with(user_pattern) {
                        suggestions.push(UnprocessedSuggestion::Ready(Suggestion::new(
                            format!("~{}/", name_str),
                            "",
                            "",
                        )));
                    }
                }
            }
        }

        // Also check root (whose home is /root, not under /home/)
        if "root".starts_with(user_pattern)
            && Path::new("/root").is_dir()
            && !suggestions.iter().any(|s| s.match_text() == "~root/")
        {
            suggestions.push(UnprocessedSuggestion::Ready(Suggestion::new(
                "~root/", "", "",
            )));
        }

        suggestions.sort_by(|a, b| a.match_text().cmp(b.match_text()));
        suggestions
    }

    #[cfg(feature = "integration-tests")]
    pub fn test_tab_completions(&mut self) {
        use crate::logging;
        use core::panic;
        use itertools::Itertools;

        log::set_max_level(log::LevelFilter::Debug);
        logging::stream_logs("stderr".into()).unwrap();

        let mut run_test_on = |command: &str, expected_suggestions: &[&Suggestion]| {
            log::info!(
                "\n\n---------------------------------------------------------------------------------"
            );
            log::info!("Testing tab completion for command: '{}'", command);
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

            let mut suggestions: Vec<Suggestion> = some_suggestions
                .unwrap()
                .iter()
                .map(|item| item.to_suggestion())
                .collect();

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

        let cwd = std::env::current_dir().unwrap();
        log::info!("Current directory: {:?}", cwd);

        if let Ok(entries) = std::fs::read_dir(&cwd) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    let file_type = entry.file_type().unwrap();
                    if file_type.is_dir() {
                        log::info!("DIR: {:?}", path);
                    } else if file_type.is_file() {
                        log::info!("FILE: {:?}", path);
                    } else {
                        log::info!("OTHER: {:?}", path);
                    }
                }
            }
        }

        run_test_on(
            "fl_comp_util --filenames ",
            &[
                &Suggestion::new(r#"abc/"#, "", ""),
                &Suggestion::new(r#"bar.txt"#, "", " "),
                &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &Suggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --quoting-desired ",
            &[&Suggestion::new(r#"multi\ word\ option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-quote ",
            &[&Suggestion::new(r#"multi word option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --dont-suppress-append ",
            &[&Suggestion::new(r#"foo"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-append ",
            &[&Suggestion::new(r#"foo"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_default_filenames --fallback-to-default man",
            &[
                // &Suggestion::new(r#"bar.txt"#, "", " "),
                // &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                // &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --fallback-to-default $FOOBARBA",
            &[&Suggestion::new(r#"$FOOBARBAZ"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ma*",
            &[&Suggestion::new(r#"many\ spaces\ here/"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_dirnames --fallback-to-default-filenames ",
            &[
                &Suggestion::new(r#"abc/"#, "", ""),
                &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &Suggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util_plusdirs --quoting-desired ",
            &[
                &Suggestion::new(r#"abc/"#, "", ""),
                &Suggestion::new(r#"foo/"#, "", ""),
                &Suggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &Suggestion::new(r#"multi\ word\ option"#, "", " "),
                &Suggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        // Test that alias expansion works: fl_comp_alias is 'fl_comp_util --nosort',
        // so completing after it should yield the same results as 'fl_comp_util --nosort '.
        run_test_on(
            "fl_comp_alias ",
            &[
                &Suggestion::new(r#"apple"#, "", " "),
                &Suggestion::new(r#"banana"#, "", " "),
                &Suggestion::new(r#"cherry"#, "", " "),
            ],
        );

        // Test that we don't quote the prefix but do quote the part of the path filled in by tab completion
        run_test_on(
            "fl_comp_util --fallback-to-default $PWD/man",
            &[&Suggestion::new(r#"$PWD/many\ spaces\ here/"#, "", "")],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default $PWD/many\ spac"#,
            &[&Suggestion::new(r#"$PWD/many\ spaces\ here/"#, "", "")],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default "$PWD/many spac"#,
            &[&Suggestion::new(r#""$PWD/many spaces here/"#, "", "")],
        );

        // Test that $HOME prefix is preserved (not backslash-escaped) while the
        // dollar sign in the new filename part IS escaped.
        // $HOME/foo/ should complete to $HOME/foo/\$baz.txt (not \$HOME/foo/\$baz.txt).
        run_test_on(
            "fl_comp_util --env-var-test $HOME/foo/",
            &[&Suggestion::new(r#"$HOME/foo/\$baz.txt"#, "", " ")],
        );

        // Test glob expansion with glob characters in directory components
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default foo*/ba*",
            &[&Suggestion::new(r#"foo/baz"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default abc/foo*/ba*",
            &[&Suggestion::new(r#"abc/foo/baz"#, "", " ")],
        );

        println!("Tab completion tests FLYLINE_TEST_SUCCESS");
    }
}
