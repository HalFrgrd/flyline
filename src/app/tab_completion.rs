use crate::active_suggestions::{
    ActiveSuggestions, MaybeProcessedSuggestion, ProcssedSuggestion, SuggestionDescription,
};
use crate::app::{App, ContentMode, TabCompletionHandle};
use crate::bash_funcs::{self, QuoteType};
use crate::text_buffer::SubString;
use crate::users;
use crate::{complete_flyline_args, tab_completion_context};
use glob;

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
        let expanded_prefix = bash_funcs::fully_expand_path(&raw_prefix);

        let rhs_pattern = bash_funcs::dequoting_function_rust(&rhs_pattern);

        PathPatternExpansion {
            raw_prefix,
            expanded_prefix,
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

    fn prefix_with_trailing_slash(&self) -> String {
        let mut prefix = self.expanded_prefix.clone();
        if prefix.is_empty() {
            return prefix;
        }
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        prefix
    }

    fn wants_hidden(&self) -> bool {
        self.rhs_pattern.starts_with('.') && !self.rhs_pattern.starts_with("./")
    }

    fn convert_expanded_match_to_unexpanded(
        &self,
        expanded_match: &str,
        quote_type: Option<QuoteType>,
    ) -> (String, String) {
        // Compute the relative path of the result compared to
        // expanded_prefix, then reconstruct using raw_prefix so the
        // suggestion preserves the user's original prefix spelling
        // (e.g. `~/`, `$HOME/`, or a relative path segment).
        if let Some(suffix) = expanded_match.strip_prefix(&self.prefix_with_trailing_slash()) {
            let quoted_suffix = bash_funcs::quoting_function_rust(
                suffix,
                quote_type.unwrap_or_default(),
                false,
                false,
            );
            if self.raw_prefix.is_empty() {
                (quoted_suffix.clone(), quoted_suffix)
            } else {
                let combined = format!("{}/{}", self.raw_prefix, quoted_suffix);
                (combined.clone(), quoted_suffix)
            }
        } else {
            log::warn!(
                "Expected expanded match '{}' to start with expanded_prefix '{}', but it did not.",
                expanded_match,
                self.prefix_with_trailing_slash()
            );
            (expanded_match.to_string(), expanded_match.to_string())
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
    word_under_cursor: &SubString,
    context: &str,
    context_until_cursor: &str,
) -> AliasExpandedCompletion {
    // Capture the original length before potentially moving `command_word`.
    let command_word_len = command_word.len();

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
        command_word
    };

    let len_delta = alias.len() as isize - command_word_len as isize;
    let word_under_cursor_end = word_under_cursor.end().saturating_add_signed(len_delta);

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

/// Find the longest string that is a prefix of every suggestion's match text.
/// Returns `None` if there are no suggestions or the common prefix is empty.
fn common_prefix_of_suggestions(suggestions: &[MaybeProcessedSuggestion]) -> Option<String> {
    let mut iter = suggestions.iter();
    let first_text = iter.next()?.match_text();

    let prefix_byte_len = iter.fold(first_text.len(), |acc, sug| {
        let text = sug.match_text();
        let common: usize = first_text
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum();
        acc.min(common)
    });

    if prefix_byte_len == 0 {
        None
    } else {
        Some(first_text[..prefix_byte_len].to_string())
    }
}

fn post_process_completions(
    completions: Vec<String>,
    comp_resultflags: bash_funcs::CompletionFlags,
    word_under_cursor: &str,
) -> Vec<MaybeProcessedSuggestion> {
    completions
        .into_iter()
        .map(|sug| MaybeProcessedSuggestion::Raw {
            raw_text: sug,
            full_path: None,
            flags: comp_resultflags,
            word_under_cursor: word_under_cursor.to_string(),
        })
        .collect()
}

pub(crate) fn gen_completions_internal(
    completion_context: &tab_completion_context::CompletionContext,
) -> Option<Vec<MaybeProcessedSuggestion>> {
    log::debug!("Completion context: {:#?}", completion_context);

    let word_under_cursor = &completion_context.word_under_cursor;

    match &completion_context.comp_type {
        tab_completion_context::CompType::FirstWord => {
            let completions = tab_complete_first_word(word_under_cursor.as_ref());
            log::debug!("Primary completions for first word: {:?}", completions);
            if !completions.is_empty() {
                return Some(completions);
            }
        }
        tab_completion_context::CompType::CommandComp {
            command_word: initial_command_word,
        } => {
            // This isn't just for commands like `git`, `cargo`
            // Because we call bash_symbols::programmable_completions
            // Bash also completes env vars (`echo $HO`) and other useful completions.
            // Bash doesn't handle alias expansion well:
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
                completion_context.context.as_ref(),
                completion_context.context_until_cursor.as_ref(),
            );

            let poss_completions = if command_word == "flyline" {
                // Flyline's own subcommand/flag completions are produced by
                // clap_complete and are already escaped/finalized. Skip the
                // bash post-processing pipeline entirely and build
                // ProcssedSuggestions directly so descriptions (the help text
                // attached to each candidate) are preserved as-is.
                match complete_flyline_args(&full_command, cursor_byte_pos) {
                    Ok(candidates) => {
                        let suggestions: Vec<MaybeProcessedSuggestion> = candidates
                            .into_iter()
                            .map(|c| {
                                let value = c.get_value().to_string_lossy().to_string();
                                let help = c
                                    .get_help()
                                    .map(|h| h.to_string())
                                    .filter(|h| !h.is_empty());
                                let description = match help {
                                    Some(h) => SuggestionDescription::Animation(vec![h]),
                                    None => SuggestionDescription::Static(String::new()),
                                };
                                MaybeProcessedSuggestion::Ready(
                                    ProcssedSuggestion::new(&value, "", "")
                                        .with_description(description),
                                )
                            })
                            .collect();
                        return Some(suggestions);
                    }
                    Err(e) => {
                        log::error!("Error generating flyline completions: {}", e);
                        return None;
                    }
                }
            } else {
                bash_funcs::run_programmable_completions(
                    &full_command,
                    &command_word,
                    word_under_cursor.as_ref(),
                    cursor_byte_pos,
                    word_under_cursor_end,
                )
            };

            match poss_completions {
                Ok(comp_result) if !comp_result.completions.is_empty() => {
                    log::debug!(
                        "Programmable completion results for command: {}",
                        full_command
                    );
                    log::debug!("Completions: {:#?}", comp_result);

                    let suggestions = post_process_completions(
                        comp_result.completions,
                        comp_result.flags,
                        word_under_cursor.as_ref(),
                    );
                    return Some(suggestions);
                }
                Ok(comp_result) => {
                    // I am not checking if the user wants more completions (i.e. readline_default_fallback_desired)
                    // Always try to produce secondary completions
                    return gen_secondary_completions(completion_context, comp_result.flags);
                }
                _ => {}
            }
        }
    }

    log::debug!(
        "No programmable completions found completion_context: {:#?}. Falling back to secondary completions.",
        completion_context
    );

    gen_secondary_completions(completion_context, bash_funcs::CompletionFlags::default())
}

fn gen_secondary_completions(
    completion_context: &tab_completion_context::CompletionContext,
    comp_resultflags: bash_funcs::CompletionFlags,
) -> Option<Vec<MaybeProcessedSuggestion>> {
    let word_under_cursor = completion_context.word_under_cursor.as_ref();
    match completion_context.comp_type_secondary {
        Some(tab_completion_context::SecondaryCompType::EnvVariable) => {
            log::debug!("Environment variable completion {:?}", word_under_cursor);
            let matching_vars = bash_funcs::get_all_variables_with_prefix(word_under_cursor);
            return Some(
                ProcssedSuggestion::from_string_vec(matching_vars, "", " ")
                    .into_iter()
                    .map(MaybeProcessedSuggestion::Ready)
                    .collect(),
            );
        }
        Some(tab_completion_context::SecondaryCompType::TildeExpansion) => {
            log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
            let completions = tab_complete_tilde_expansion(word_under_cursor);
            return Some(completions);
        }
        Some(tab_completion_context::SecondaryCompType::GlobExpansion) => {
            log::debug!("Glob expansion for: {:?}", word_under_cursor);
            let completions = tab_complete_glob_expansion(word_under_cursor, comp_resultflags);

            // Unlike other completions, if there are multiple glob completions,
            // we join them with spaces and insert them all at once.
            // Process each item eagerly here since we need the final text.
            let completions_as_string = completions
                .into_iter()
                .map(|mut item| item.to_suggestion().s)
                .fold(String::new(), |mut acc, s| {
                    if !acc.is_empty() {
                        if comp_resultflags.quote_type == Some(QuoteType::DoubleQuote) {
                            acc.push_str("\" ");
                        } else if comp_resultflags.quote_type == Some(QuoteType::SingleQuote) {
                            acc.push_str("' ");
                        } else {
                            acc.push(' ');
                        }
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
                    ProcssedSuggestion::from_string_vec(vec![completions_as_string], "", suffix)
                        .into_iter()
                        .map(MaybeProcessedSuggestion::Ready)
                        .collect(),
                );
            }
        }
        Some(tab_completion_context::SecondaryCompType::FilenameExpansion) => {
            log::debug!("Filename expansion for: {:?}", word_under_cursor);
            let completions = tab_complete_glob_expansion(
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

fn tab_complete_first_word(command: &str) -> Vec<MaybeProcessedSuggestion> {
    log::debug!("Generating first word completions for: '{}'", command);
    if command.is_empty() {
        return vec![];
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        // Path to executable
        return tab_complete_glob_expansion(
            &(command.to_string() + "*"),
            bash_funcs::CompletionFlags::default(),
        );
    }

    let mut res = bash_funcs::get_first_word_completions(command);

    if res.is_empty() {
        // No prefix matches found, fall back to fuzzy search
        log::debug!("No prefix matches for '{}', trying fuzzy search", command);
        res = bash_funcs::get_fuzzy_first_word_completions(command);
        return ProcssedSuggestion::from_string_vec(res, "", " ")
            .into_iter()
            .map(MaybeProcessedSuggestion::Ready)
            .collect();
    }

    // TODO: could prioritize based on frequency of use
    res.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    res.dedup();
    ProcssedSuggestion::from_string_vec(res, "", " ")
        .into_iter()
        .map(MaybeProcessedSuggestion::Ready)
        .collect()
}

fn tab_complete_glob_expansion(
    pattern: &str,
    mut comp_resultflags: bash_funcs::CompletionFlags,
) -> Vec<MaybeProcessedSuggestion> {
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

    let glob_pattern = expanded.expanded_pattern();

    if let Ok(paths) = glob::glob(&glob_pattern) {
        for (idx, path_result) in paths.enumerate() {
            if idx >= MAX_GLOB_RESULTS {
                log::debug!(
                    "Reached maximum glob results limit of {}. Stopping further processing.",
                    MAX_GLOB_RESULTS
                );
                break;
            }

            if let Ok(path) = path_result {
                let (unexpanded, globbed_suffix) = expanded.convert_expanded_match_to_unexpanded(
                    &path.to_string_lossy(),
                    comp_resultflags.quote_type,
                );

                log::debug!(
                    "Glob match: expanded='{}', unexpanded='{}', globbed_suffix='{}'",
                    path.display(),
                    unexpanded,
                    globbed_suffix
                );

                // Tab completion ignores "." and ".."
                if globbed_suffix == "." || globbed_suffix == ".." {
                    continue;
                }

                // Only include hidden if the pattern explicitly requested it
                if !expanded.wants_hidden()
                    && globbed_suffix.starts_with('.')
                    && !globbed_suffix.starts_with("./")
                {
                    continue;
                }

                results.push(MaybeProcessedSuggestion::Raw {
                    raw_text: unexpanded,
                    full_path: Some(path),
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

fn tab_complete_tilde_expansion(pattern: &str) -> Vec<MaybeProcessedSuggestion> {
    let user_pattern = if let Some(stripped) = pattern.strip_prefix('~') {
        stripped
    } else {
        return vec![];
    };

    // `~` alone — suggest the current user's home directory as `~/`
    if user_pattern.is_empty() {
        return vec![MaybeProcessedSuggestion::Ready(ProcssedSuggestion::new(
            "~/", "", "",
        ))];
    }

    // `~username` — find matching users from the users module
    let mut suggestions = Vec::new();

    for user in users::get_all_users() {
        if user.username.starts_with(user_pattern) {
            suggestions.push(MaybeProcessedSuggestion::Ready(ProcssedSuggestion::new(
                format!("{}/", user.home_dir),
                "",
                "",
            )));
        }
    }

    suggestions.sort_by(|a, b| a.match_text().cmp(b.match_text()));
    suggestions
}

impl App<'_> {
    fn try_accept_tab_completion(&mut self, suggs: ActiveSuggestions) {
        match suggs.try_accept(&mut self.buffer) {
            None => {
                self.content_mode = ContentMode::Normal;
            }
            Some(suggestions) => {
                self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
            }
        }
    }

    /// Apply the results of tab completion generation (Phase 2 & 3: common
    /// prefix insertion and handing suggestions to the UI).
    pub(crate) fn finish_tab_complete(
        &mut self,
        sugs: Vec<MaybeProcessedSuggestion>,
        wuc_substring: SubString,
    ) {
        let mut final_wuc = wuc_substring.clone();
        // Phase 2: if there are fewer than 500 suggestions, find any common
        // prefix and automatically insert it when it extends the word under
        // cursor.
        const MAX_FOR_COMMON_PREFIX: usize = 500;
        if sugs.len() < MAX_FOR_COMMON_PREFIX {
            if let Some(common_prefix) = common_prefix_of_suggestions(&sugs) {
                if common_prefix.len() > wuc_substring.s.len()
                    && common_prefix.starts_with(&*wuc_substring.s)
                {
                    match self
                        .buffer
                        .replace_word_under_cursor(&common_prefix, &wuc_substring)
                    {
                        Ok(new_wuc) => {
                            log::info!(
                                "New word under cursor after inserting common prefix: '{:?}'",
                                new_wuc
                            );
                            final_wuc = new_wuc;
                        }
                        Err(e) => log::warn!(
                            "Failed to replace word under cursor with common prefix: {}",
                            e
                        ),
                    }
                }
            }
        }

        // Phase 3: hand the suggestions off to the UI layer.
        self.try_accept_tab_completion(ActiveSuggestions::new(sugs, final_wuc));
    }

    pub fn start_tab_complete(&mut self) {
        // Phase 1: compute the completion context and generate suggestions.
        // We store word_under_cursor as an owned SubString so we can use it
        // after the immutable-borrow block ends.

        let completion_context = tab_completion_context::get_completion_context(
            self.buffer.buffer(),
            self.buffer.cursor_byte_pos(),
        );

        let wuc_substring = completion_context.word_under_cursor.clone();

        let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<MaybeProcessedSuggestion>>>();

        let completion_context_owned = completion_context.into_owned();

        let thread_handle = std::thread::spawn(move || {
            let suggestions = gen_completions_internal(&completion_context_owned);
            if suggestions.is_none() {
                log::debug!(
                    "No suggestions generated for completion context: {:?}",
                    completion_context_owned
                );
            }
            if let Err(e) = tx.send(suggestions) {
                log::warn!(
                    "Tab completion: failed to send result (receiver dropped): {:?}",
                    e
                );
            }
        });

        // Block for up to 100ms waiting for the thread to finish.
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Some(sugs)) => {
                self.finish_tab_complete(sugs, wuc_substring);
            }
            Ok(None) => {
                // No suggestions generated.
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Thread hasn't finished yet; enter waiting mode.
                self.content_mode = ContentMode::TabCompletionWaiting {
                    handle: TabCompletionHandle {
                        receiver: rx,
                        thread: Some(thread_handle),
                    },
                    wuc_substring,
                };
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("Tab completion thread disconnected unexpectedly");
            }
        }
    }

    #[cfg(feature = "integration-tests")]
    pub fn test_tab_completions(&mut self) {
        use crate::logging;
        use core::panic;
        use itertools::Itertools;

        log::set_max_level(log::LevelFilter::Debug);
        logging::stream_logs("stderr".into()).unwrap();

        let mut run_test_on = |command: &str, expected_suggestions: &[&ProcssedSuggestion]| {
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
            let some_suggestions = gen_completions_internal(&comp_context);

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

            let mut suggestions: Vec<ProcssedSuggestion> = some_suggestions
                .unwrap()
                .iter_mut()
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
                            (&sug.prefix, &sug.s, &sug.suffix),
                            (&expected.prefix, &expected.s, &expected.suffix),
                            "For command '{}', expected suggestion '{:?}' but got '{:?}'",
                            command,
                            expected,
                            sug
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
                &ProcssedSuggestion::new(r#"abc/"#, "", ""),
                &ProcssedSuggestion::new(r#"bar.txt"#, "", " "),
                &ProcssedSuggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &ProcssedSuggestion::new(r#"foo/"#, "", ""),
                &ProcssedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcssedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --quoting-desired ",
            &[&ProcssedSuggestion::new(r#"multi\ word\ option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-quote ",
            &[&ProcssedSuggestion::new(r#"multi word option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --dont-suppress-append ",
            &[&ProcssedSuggestion::new(r#"foo"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-append ",
            &[&ProcssedSuggestion::new(r#"foo"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_default_filenames --fallback-to-default man",
            &[
                // &Suggestion::new(r#"bar.txt"#, "", " "),
                // &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                // &Suggestion::new(r#"foo/"#, "", ""),
                &ProcssedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --fallback-to-default $FOOBARBA",
            &[&ProcssedSuggestion::new(r#"$FOOBARBAZ"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ma*",
            &[&ProcssedSuggestion::new(r#"many\ spaces\ here/"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_dirnames --fallback-to-default-filenames ",
            &[
                &ProcssedSuggestion::new(r#"abc/"#, "", ""),
                &ProcssedSuggestion::new(r#"foo/"#, "", ""),
                &ProcssedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcssedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util_plusdirs --quoting-desired ",
            &[
                &ProcssedSuggestion::new(r#"abc/"#, "", ""),
                &ProcssedSuggestion::new(r#"foo/"#, "", ""),
                &ProcssedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcssedSuggestion::new(r#"multi\ word\ option"#, "", " "),
                &ProcssedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        // Test that alias expansion works: fl_comp_alias is 'fl_comp_util --nosort',
        // so completing after it should yield the same results as 'fl_comp_util --nosort '.
        run_test_on(
            "fl_comp_alias ",
            &[
                &ProcssedSuggestion::new(r#"apple"#, "", " "),
                &ProcssedSuggestion::new(r#"banana"#, "", " "),
                &ProcssedSuggestion::new(r#"cherry"#, "", " "),
            ],
        );

        // Test that we don't quote the prefix but do quote the part of the path filled in by tab completion
        run_test_on(
            "fl_comp_util --fallback-to-default $PWD/man",
            &[&ProcssedSuggestion::new(
                r#"$PWD/many\ spaces\ here/"#,
                "",
                "",
            )],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default $PWD/many\ spac"#,
            &[&ProcssedSuggestion::new(
                r#"$PWD/many\ spaces\ here/"#,
                "",
                "",
            )],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default "$PWD/many spac"#,
            &[&ProcssedSuggestion::new(
                r#""$PWD/many spaces here/"#,
                "",
                "",
            )],
        );

        // Test that $HOME prefix is preserved (not backslash-escaped) while the
        // dollar sign in the new filename part IS escaped.
        // $HOME/foo/ should complete to $HOME/foo/\$baz.txt (not \$HOME/foo/\$baz.txt).
        run_test_on(
            "fl_comp_util --env-var-test $HOME/foo/",
            &[&ProcssedSuggestion::new(r#"$HOME/foo/\$baz.txt"#, "", " ")],
        );

        // Test glob expansion with glob characters in directory components
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default foo*/ba*",
            &[&ProcssedSuggestion::new(r#"foo/baz"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default abc/foo*/ba*",
            &[&ProcssedSuggestion::new(r#"abc/foo/baz"#, "", " ")],
        );

        // move to foo/glob_stuff dir:
        std::env::set_current_dir("/tmp/example_fs/foo/glob_stuff").unwrap();

        // .* matches hidden files only. and should ignore . and ..
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default .*",
            &[&ProcssedSuggestion::new(r#".dotfile"#, "", " ")],
        );

        // ./.* matches hidden files only. and should ignore . and ..
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ./.*",
            &[&ProcssedSuggestion::new(r#"./.dotfile"#, "", " ")],
        );

        // ./* matches all non hidden
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ./*",
            &[&ProcssedSuggestion::new(r#"./a.txt"#, "", " ")],
        );

        // * matches all non hidden
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default *",
            &[&ProcssedSuggestion::new(r#"a.txt"#, "", " ")],
        );

        println!("Tab completion tests FLYLINE_TEST_SUCCESS");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ready(s: &str) -> MaybeProcessedSuggestion {
        MaybeProcessedSuggestion::Ready(ProcssedSuggestion::new(s, "", ""))
    }

    #[test]
    fn common_prefix_empty_slice() {
        assert_eq!(common_prefix_of_suggestions(&[]), None);
    }

    #[test]
    fn common_prefix_single_suggestion() {
        let sugs = vec![make_ready("foobar")];
        assert_eq!(
            common_prefix_of_suggestions(&sugs),
            Some("foobar".to_string())
        );
    }

    #[test]
    fn common_prefix_identical_suggestions() {
        let sugs = vec![make_ready("abc"), make_ready("abc"), make_ready("abc")];
        assert_eq!(common_prefix_of_suggestions(&sugs), Some("abc".to_string()));
    }

    #[test]
    fn common_prefix_shared_prefix() {
        let sugs = vec![
            make_ready("foobar"),
            make_ready("foobaz"),
            make_ready("foo"),
        ];
        assert_eq!(common_prefix_of_suggestions(&sugs), Some("foo".to_string()));
    }

    #[test]
    fn common_prefix_no_shared_prefix() {
        let sugs = vec![make_ready("apple"), make_ready("banana")];
        assert_eq!(common_prefix_of_suggestions(&sugs), None);
    }

    #[test]
    fn common_prefix_unicode() {
        let sugs = vec![make_ready("café_au_lait"), make_ready("café_crème")];
        assert_eq!(
            common_prefix_of_suggestions(&sugs),
            Some("café_".to_string())
        );
    }

    #[test]
    fn common_prefix_raw_suggestions() {
        let flags = bash_funcs::CompletionFlags::default();
        let sugs = vec![
            MaybeProcessedSuggestion::Raw {
                raw_text: "git-commit".to_string(),
                full_path: None,
                flags,
                word_under_cursor: "git".to_string(),
            },
            MaybeProcessedSuggestion::Raw {
                raw_text: "git-checkout".to_string(),
                full_path: None,
                flags,
                word_under_cursor: "git".to_string(),
            },
        ];
        // "git-commit" and "git-checkout" share "git-c" before diverging
        assert_eq!(
            common_prefix_of_suggestions(&sugs),
            Some("git-c".to_string())
        );
    }
}
