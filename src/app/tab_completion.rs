use crate::active_suggestions::{
    ActiveSuggestions, ActiveSuggestionsBuilder, ProcessedSuggestion, SuggestionDescription,
    UnprocessedSuggestion,
};
use crate::app::{App, ContentMode, TabCompletionHandle};
use crate::bash_funcs::{self, QuoteType};
use crate::content_utils::{ansi_string_to_spans, easing_animation_frames};
use crate::cursor::{CursorEasing, cursor_effect_animation_frames};
use crate::globbing::PathPatternExpansion;
use crate::iter_first_last::FirstLast;
use crate::text_buffer::SubString;
use crate::users;
use crate::{complete_flyline_args, tab_completion_context};
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;

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

pub(crate) fn gen_completions_internal(
    completion_context: &tab_completion_context::CompletionContext,
    cursor_config: &crate::cursor::CursorConfig,
) -> Option<ActiveSuggestionsBuilder> {
    log::debug!("Completion context: {:#?}", completion_context);

    let word_under_cursor = &completion_context.word_under_cursor;

    let mut comp_res_flags = bash_funcs::CompletionFlags::default();

    for comp_type in &completion_context.comp_types {
        log::debug!("Processing completion type: {:?}", comp_type);
        match comp_type {
            tab_completion_context::CompType::FirstWord => {
                log::debug!("First word completion for: {:?}", word_under_cursor);
                let completions = tab_complete_first_word(word_under_cursor.as_ref());
                if completions.is_empty() {
                    log::debug!(
                        "No first word completions found for prefix: {}",
                        word_under_cursor.as_ref()
                    );
                } else {
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

                if command_word == "flyline" {
                    // Flyline's own subcommand/flag completions are produced by
                    // clap_complete and are already escaped/finalized. Skip the
                    // bash post-processing pipeline entirely and build
                    // ProcssedSuggestions directly so descriptions (the help text
                    // attached to each candidate) are preserved as-is.
                    match complete_flyline_args(
                        &full_command,
                        word_under_cursor.as_ref(),
                        cursor_byte_pos,
                    ) {
                        Ok(candidates) if !candidates.is_empty() => {
                            let preceding_flag = &full_command[..cursor_byte_pos]
                                .split_whitespace()
                                .rfind(|w| w.starts_with("--"));

                            let quote_type =
                                bash_funcs::find_quote_type(word_under_cursor.as_ref());

                            let mut builder = ActiveSuggestionsBuilder::new();
                            builder.extend_processed(candidates.into_iter().map(|c| {
                                let value = c.get_value().to_string_lossy().to_string();
                                let value = if let Some(qt) = quote_type {
                                    bash_funcs::quoting_function_rust(&value, qt, true, false)
                                } else {
                                    value.clone()
                                };

                                let (value, suffix) =
                                    if let Some(stripped) = value.strip_suffix("NO_SUFFIX") {
                                        (stripped.to_string(), "")
                                    } else {
                                        (value, " ")
                                    };
                                let (prefix, value) = if let Some(delim_pos) =
                                    value.find("PREFIX_DELIM")
                                {
                                    let p = value[..delim_pos].to_string();
                                    let v = value[delim_pos + "PREFIX_DELIM".len()..].to_string();
                                    (p, v)
                                } else {
                                    (String::new(), value)
                                };

                                let help_description = || {
                                    let help = c
                                        .get_help()
                                        .map(|h| h.to_string())
                                        .filter(|h| !h.is_empty());
                                    match help {
                                        Some(h) => SuggestionDescription::Animation(vec![
                                            ansi_string_to_spans(&h),
                                        ]),
                                        None => SuggestionDescription::Static(vec![]),
                                    }
                                };
                                let description = match (
                                    preceding_flag,
                                    CursorEasing::try_from_value_name(&value),
                                ) {
                                    (Some("--effect-easing"), Some(easing)) => {
                                        SuggestionDescription::Animation(
                                            cursor_effect_animation_frames(
                                                easing,
                                                cursor_config.effect_speed,
                                            ),
                                        )
                                    }
                                    (Some("--interpolate-easing"), Some(easing)) => {
                                        SuggestionDescription::Animation(easing_animation_frames(
                                            easing,
                                        ))
                                    }
                                    _ => help_description(),
                                };

                                ProcessedSuggestion::new(&value, prefix, suffix)
                                    .with_description(description)
                            }));
                            return Some(builder);
                        }
                        Ok(_) => {
                            log::debug!(
                                "No flyline completions found for command '{}'",
                                full_command
                            );
                        }
                        Err(e) => {
                            log::error!("Error generating flyline completions: {}", e);
                        }
                    }
                } else {
                    let poss_completions = bash_funcs::run_programmable_completions(
                        &full_command,
                        &command_word,
                        word_under_cursor.as_ref(),
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

                            let mut builder = ActiveSuggestionsBuilder::new();
                            builder.extend_unprocessed(comp_result.completions.into_iter().map(
                                |sug| UnprocessedSuggestion {
                                    raw_text: sug,
                                    full_path: None,
                                    flags: comp_result.flags,
                                    word_under_cursor: word_under_cursor.as_ref().to_string(),
                                },
                            ));
                            return Some(builder);
                        }
                        Ok(comp_result) => {
                            // I am not checking if the user wants more completions (i.e. readline_default_fallback_desired)
                            // Always try to produce secondary completions
                            comp_res_flags = comp_result.flags;
                        }
                        _ => {}
                    }
                }
            }

            tab_completion_context::CompType::EnvVariable => {
                log::debug!("Environment variable completion {:?}", word_under_cursor);
                let matching_vars =
                    bash_funcs::get_all_variables_with_prefix(word_under_cursor.as_ref());
                if matching_vars.is_empty() {
                    log::debug!(
                        "No environment variable completions found for prefix: {}",
                        word_under_cursor.as_ref()
                    );
                } else {
                    let mut builder = ActiveSuggestionsBuilder::new();
                    builder.extend_processed(ProcessedSuggestion::from_string_vec(
                        matching_vars,
                        "",
                        " ",
                    ));
                    return Some(builder);
                }
            }
            tab_completion_context::CompType::TildeExpansion => {
                log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
                let completions = tab_complete_tilde_expansion(word_under_cursor.as_ref());
                if completions.is_empty() {
                    log::debug!(
                        "No tilde expansion completions found for pattern: {}",
                        word_under_cursor.as_ref()
                    );
                } else {
                    let mut builder = ActiveSuggestionsBuilder::new();
                    builder.extend_processed(completions);
                    return Some(builder);
                }
            }
            tab_completion_context::CompType::GlobExpansion => {
                log::debug!("Glob expansion for: {:?}", word_under_cursor);
                let completions =
                    tab_complete_glob_expansion(word_under_cursor.as_ref(), comp_res_flags);

                match completions.as_slice() {
                    [] => {
                        log::debug!(
                            "No glob expansion completions found for pattern: {}",
                            word_under_cursor.as_ref()
                        );
                    }
                    [single_completion] => {
                        let processed = single_completion.clone().into_processed();
                        log::debug!(
                            "Only one glob expansion completion found for pattern '{}': '{:?}'",
                            word_under_cursor.as_ref(),
                            processed
                        );
                        let mut builder = ActiveSuggestionsBuilder::new();
                        builder.push_processed(processed);
                        return Some(builder);
                    }
                    _ => {
                        log::debug!(
                            "Multiple glob expansion completions found for pattern '{}': {:#?}",
                            word_under_cursor.as_ref(),
                            completions.iter().take(20)
                        );
                        // Unlike other completions, if there are multiple glob completions,
                        // we join them with spaces and insert them all at once.
                        // Process each item eagerly here since we need the final text.
                        let completions_as_string = completions.into_iter().flag_first_last().fold(
                            String::new(),
                            |mut acc, (is_first, is_last, item)| {
                                let sug = item.into_processed();
                                if !is_first {
                                    acc.push(' ');
                                }

                                match comp_res_flags.quote_type {
                                    Some(QuoteType::DoubleQuote) => acc.push_str("\""),
                                    Some(QuoteType::SingleQuote) => acc.push_str("'"),
                                    _ => {}
                                }
                                acc.push_str(&sug.s);

                                if !is_last {
                                    match comp_res_flags.quote_type {
                                        Some(QuoteType::DoubleQuote) => acc.push_str("\""),
                                        Some(QuoteType::SingleQuote) => acc.push_str("'"),
                                        _ => {}
                                    }
                                } else {
                                    acc.push_str(&sug.suffix);
                                }

                                acc
                            },
                        );
                        let mut builder = ActiveSuggestionsBuilder::new();
                        builder.push_processed(ProcessedSuggestion::new(
                            completions_as_string,
                            "",
                            "",
                        ));
                        return Some(builder);
                    }
                }
            }
            tab_completion_context::CompType::FilenameExpansion => {
                log::debug!("Filename expansion for: {:?}", word_under_cursor);
                let completions = tab_complete_glob_expansion(
                    &(word_under_cursor.as_ref().to_string() + "*"),
                    comp_res_flags,
                );

                if completions.is_empty() {
                    log::debug!(
                        "No filename expansion completions found for pattern: {}",
                        word_under_cursor.as_ref()
                    );
                } else {
                    let mut builder = ActiveSuggestionsBuilder::new();
                    builder.extend_unprocessed(completions);
                    return Some(builder);
                }
            }
            tab_completion_context::CompType::FuzzyFilenameExpansion => {
                log::debug!("Fuzzy filename expansion for: {:?}", word_under_cursor);
                let completions =
                    tab_complete_fuzzy_filename(word_under_cursor.as_ref(), comp_res_flags);

                if completions.is_empty() {
                    log::debug!(
                        "No fuzzy filename completions found for: {}",
                        word_under_cursor.as_ref()
                    );
                } else {
                    let mut builder =
                        ActiveSuggestionsBuilder::new().with_auto_accept_if_solo(false);
                    builder.extend_unprocessed(completions);
                    return Some(builder);
                }
            }
        }
    }

    // gen_secondary_completions(completion_context, bash_funcs::CompletionFlags::default())
    log::debug!("No completion types produced result");
    None
}

fn tab_complete_first_word(command: &str) -> ActiveSuggestionsBuilder {
    log::debug!("Generating first word completions for: '{}'", command);
    let mut builder = ActiveSuggestionsBuilder::new();
    if command.is_empty() {
        return builder;
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        // Path to executable
        builder.extend_unprocessed(tab_complete_glob_expansion(
            &(command.to_string() + "*"),
            bash_funcs::CompletionFlags::default(),
        ));
        return builder;
    }

    let mut res = bash_funcs::get_first_word_completions(command);

    if res.is_empty() {
        // No prefix matches found, fall back to fuzzy search
        log::debug!("No prefix matches for '{}', trying fuzzy search", command);
        res = bash_funcs::get_fuzzy_first_word_completions(command);
        builder.extend_processed(ProcessedSuggestion::from_string_vec(res, "", " "));
        return builder;
    }

    // TODO: could prioritize based on frequency of use
    res.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    res.dedup();
    builder.extend_processed(ProcessedSuggestion::from_string_vec(res, "", " "));
    builder
}

/// Core glob expansion logic that works with an already-expanded PathPatternExpansion.
/// This is the common logic used by both prefix-matching and fuzzy-filename completion paths.
///
/// `should_skip_hidden`: If true, skip files starting with `.` (unless pattern explicitly requests them).
fn tab_complete_with_expanded_pattern(
    expanded: &PathPatternExpansion,
    comp_resultflags: bash_funcs::CompletionFlags,
    should_skip_hidden: bool,
) -> Vec<UnprocessedSuggestion> {
    
    let mut results = Vec::new();
    
    const MAX_GLOB_RESULTS: usize = 5_000;
    
    let glob_patterns = expanded.glob_pattern();
    
    log::debug!("Performing glob expansion for expanded: {:#?}", expanded);
    log::debug!("Using glob_patterns {:?}", glob_patterns);

    'outer: for glob_pattern in &glob_patterns {
        let Ok(paths) = glob::glob(glob_pattern) else {
            continue;
        };
        for path in paths.filter_map(Result::ok) {
            if results.len() >= MAX_GLOB_RESULTS {
                log::debug!(
                    "Reached maximum glob results limit of {}. Stopping further processing.",
                    MAX_GLOB_RESULTS
                );
                break 'outer;
            }

            let path_str = path.to_string_lossy();

            let (unexpanded, quoted_rhs) = expanded
                .convert_expanded_match_to_unexpanded(&path_str, comp_resultflags.quote_type);

            log::debug!(
                "Glob match: expanded='{}', unexpanded='{}', quoted_rhs='{}'",
                path.display(),
                unexpanded,
                quoted_rhs
            );

            // Tab completion ignores "." and ".."
            if quoted_rhs == "." || quoted_rhs == ".." {
                continue;
            }

            // Only include hidden if filtering is desired and the pattern doesn't explicitly want them
            if should_skip_hidden
                && !expanded.wants_hidden()
                && quoted_rhs.starts_with('.')
                && !quoted_rhs.starts_with("./")
            {
                continue;
            }

            results.push(UnprocessedSuggestion {
                raw_text: unexpanded,
                full_path: Some(path),
                flags: comp_resultflags,
                // The glob expansion path already preserves the raw prefix in
                // `unexpanded` via PathPatternExpansion; pass "" here so
                // into_processed doesn't attempt a second
                // prefix split (filename_quoting_desired is false anyway).
                word_under_cursor: String::new(),
            });
        }
    }

    results.sort_by(|a, b| a.match_text().cmp(b.match_text()));
    results
}

fn tab_complete_glob_expansion(
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

    tab_complete_with_expanded_pattern(&expanded, comp_resultflags, true)
}

/// List all files in the directory implied by `word_under_cursor` and return
/// those that fuzzy-match the last path segment using the Arinae matcher.
///
/// This is the fallback when [`tab_complete_glob_expansion`] (prefix matching)
/// finds no results: e.g. typing `src/tm` won't prefix-match `src/tab_completion.rs`,
/// but the fuzzy matcher will.
fn tab_complete_fuzzy_filename(
    word_under_cursor: &str,
    mut comp_res_flags: bash_funcs::CompletionFlags,
) -> Vec<UnprocessedSuggestion> {
    // Split at the last '/' to separate the directory prefix from the filename
    // fragment that will be used as the fuzzy-match pattern.

    let (dir_glob_pattern, filename_fragment) =
        if let Some(slash_pos) = word_under_cursor.rfind('/') {
            (
                word_under_cursor[..slash_pos + 1].to_string() + "*",
                word_under_cursor[slash_pos + 1..].to_string(),
            )
        } else {
            ("*".to_string(), word_under_cursor.to_string())
        };

    // Nothing to fuzzy-match against — let the caller fall through.
    if filename_fragment.is_empty() {
        return vec![];
    }


    // Set up flags for glob expansion
    comp_res_flags.filename_quoting_desired = false;
    comp_res_flags.filename_completion_desired = true;
    comp_res_flags.quote_type = bash_funcs::find_quote_type(&dir_glob_pattern);
    
    let expanded = PathPatternExpansion::new(&dir_glob_pattern);
    let all_files = tab_complete_with_expanded_pattern(&expanded, comp_res_flags, false);
    
    let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);

    // glob expansion handles dequoting the pattern, so we only need to dequote
    let dequoted_fragment = bash_funcs::dequoting_function_rust(&filename_fragment);

    let mut scored: Vec<(i64, UnprocessedSuggestion)> = all_files
        .into_iter()
        .filter_map(|sug| {
            // Match only against the last path segment so that e.g. the
            // directory prefix doesn't inflate the score.
            let match_text = sug.match_text();
            let filename = match_text.rsplit('/').next().unwrap_or(match_text);
            matcher
                .fuzzy_match(filename, &dequoted_fragment)
                .map(|score| (score, sug))
        })
        .collect();

    // Best matches first.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.dedup_by(|a, b| a.1.match_text() == b.1.match_text());
    scored.into_iter().map(|(_, sug)| sug).collect()
}

fn tab_complete_tilde_expansion(pattern: &str) -> Vec<ProcessedSuggestion> {
    let user_pattern = if let Some(stripped) = pattern.strip_prefix('~') {
        stripped
    } else {
        return vec![];
    };

    // `~username` — find matching users from the users module
    let mut suggestions = Vec::new();

    for user in users::get_all_users() {
        if user.username.starts_with(user_pattern) {
            suggestions.push(ProcessedSuggestion::new(
                if user.home_dir.ends_with('/') {
                    user.home_dir.clone()
                } else {
                    format!("{}/", user.home_dir)
                },
                "",
                "",
            ));
        }
    }

    suggestions.sort_by(|a, b| a.s.cmp(&b.s));
    suggestions.dedup_by(|a, b| a.s == b.s);
    suggestions
}

impl App<'_> {
    /// Apply the results of tab completion generation (Phase 2 & 3: common
    /// prefix insertion and handing suggestions to the UI).
    pub(crate) fn finish_tab_complete(
        &mut self,
        builder: ActiveSuggestionsBuilder,
        wuc_substring: SubString,
        load_time: std::time::Duration,
    ) {
        if builder.len() == 1
            && builder.auto_accept_if_solo
            && let Some(suggestion) = builder.processed.iter().next()
        {
            log::info!(
                "Auto-accepting solo suggestion: '{:?}' for word under cursor '{:?}'",
                suggestion,
                wuc_substring
            );
            self.buffer
                .replace_word_under_cursor(&suggestion.formatted(), &wuc_substring)
                .ok();
            self.content_mode = ContentMode::Normal;
            return;
        }

        if builder.len() == 0 {
            log::info!(
                "No suggestions generated for word under cursor '{:?}'",
                wuc_substring
            );
        }
        let mut final_wuc = wuc_substring.clone();
        // if the background thread found a common prefix, insert it.
        // e.g. ~foo<TAB> might produce /home/foobar and /home/foobaz,
        // which have common prefix /home/foo that should be inserted to aid fuzzy matching.
        if let Some(common_prefix) = builder.common_prefix.as_ref() {
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

        let suggestions = ActiveSuggestions::new(builder, final_wuc, load_time);
        self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
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

        let (tx, rx) = std::sync::mpsc::channel::<Option<ActiveSuggestionsBuilder>>();

        let completion_context_owned = completion_context.into_owned();

        let cursor_settings = self.settings.cursor_config.clone();

        let start_time = std::time::Instant::now();

        let thread_handle = std::thread::spawn(move || {
            let suggestions = gen_completions_internal(&completion_context_owned, &cursor_settings);
            if suggestions.is_none() {
                log::debug!(
                    "No suggestions generated for completion context: {:?}",
                    completion_context_owned
                );
            }
            // Auto-accept-if-solo and common-prefix insertion go hand in hand:
            // both are skipped for fuzzy filename matching so the user gets to
            // see and confirm the picked match.
            let result = suggestions.map(|mut builder| {
                let all_processed = builder.try_process_all();
                if !all_processed {
                    log::debug!(
                        "Not all suggestions were fully processed; skipping common prefix calculation"
                    );
                }
                if builder.auto_accept_if_solo && all_processed {
                    builder.set_common_prefix()
                };
                builder
            });
            if let Err(e) = tx.send(result) {
                log::warn!(
                    "Tab completion: failed to send result (receiver dropped): {:?}",
                    e
                );
            }
        });

        // Block for up to 100ms waiting for the thread to finish.
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Some(builder)) => {
                self.finish_tab_complete(builder, wuc_substring, start_time.elapsed());
            }
            Ok(None) => {
                // No suggestions generated.
                self.finish_tab_complete(
                    ActiveSuggestionsBuilder::new(),
                    wuc_substring,
                    start_time.elapsed(),
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Thread hasn't finished yet; enter waiting mode.
                self.content_mode = ContentMode::TabCompletionWaiting {
                    handle: TabCompletionHandle {
                        receiver: rx,
                        thread: Some(thread_handle),
                    },
                    wuc_substring,
                    start_time,
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

        let mut run_test_on = |command: &str, expected_suggestions: &[&ProcessedSuggestion]| {
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
            let some_suggestions =
                gen_completions_internal(&comp_context, &self.settings.cursor_config);

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

            let mut builder = some_suggestions.unwrap();
            // Drain any remaining unprocessed suggestions (the timeout in
            // try_process_all is only meaningful for the interactive path).
            while !builder.unprocessed.is_empty() {
                builder.try_process_all();
            }
            let mut suggestions: Vec<ProcessedSuggestion> = builder.processed;

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
                &ProcessedSuggestion::new(r#"abc/"#, "", ""),
                &ProcessedSuggestion::new(r#"bar.txt"#, "", " "),
                &ProcessedSuggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &ProcessedSuggestion::new(r#"foo/"#, "", ""),
                &ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcessedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --quoting-desired ",
            &[&ProcessedSuggestion::new(r#"multi\ word\ option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-quote ",
            &[&ProcessedSuggestion::new(r#"multi word option"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --dont-suppress-append ",
            &[&ProcessedSuggestion::new(r#"foo"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util --suppress-append ",
            &[&ProcessedSuggestion::new(r#"foo"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_default_filenames --fallback-to-default man",
            &[
                // &Suggestion::new(r#"bar.txt"#, "", " "),
                // &Suggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                // &Suggestion::new(r#"foo/"#, "", ""),
                &ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util --fallback-to-default $FOOBARBA",
            &[&ProcessedSuggestion::new(r#"$FOOBARBAZ"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ma*",
            &[&ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", "")],
        );

        run_test_on(
            "fl_comp_util_dirnames --fallback-to-default-filenames ",
            &[
                &ProcessedSuggestion::new(r#"abc/"#, "", ""),
                &ProcessedSuggestion::new(r#"foo/"#, "", ""),
                &ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcessedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util_plusdirs --quoting-desired ",
            &[
                &ProcessedSuggestion::new(r#"abc/"#, "", ""),
                &ProcessedSuggestion::new(r#"foo/"#, "", ""),
                &ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
                &ProcessedSuggestion::new(r#"multi\ word\ option"#, "", " "),
                &ProcessedSuggestion::new(r#"sym_link_to_foo/"#, "", ""),
            ],
        );

        // Test that alias expansion works: fl_comp_alias is 'fl_comp_util --nosort',
        // so completing after it should yield the same results as 'fl_comp_util --nosort '.
        run_test_on(
            "fl_comp_alias ",
            &[
                &ProcessedSuggestion::new(r#"apple"#, "", " "),
                &ProcessedSuggestion::new(r#"banana"#, "", " "),
                &ProcessedSuggestion::new(r#"cherry"#, "", " "),
            ],
        );

        // Test that we don't quote the prefix but do quote the part of the path filled in by tab completion
        run_test_on(
            "fl_comp_util --fallback-to-default $PWD/man",
            &[&ProcessedSuggestion::new(
                r#"$PWD/many\ spaces\ here/"#,
                "",
                "",
            )],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default $PWD/many\ spac"#,
            &[&ProcessedSuggestion::new(
                r#"$PWD/many\ spaces\ here/"#,
                "",
                "",
            )],
        );

        run_test_on(
            r#"fl_comp_util --fallback-to-default "$PWD/many spac"#,
            &[&ProcessedSuggestion::new(
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
            &[&ProcessedSuggestion::new(r#"\$baz.txt"#, "$HOME/foo/", " ")],
        );

        // Test glob expansion with glob characters in directory components
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default foo*/ba*",
            &[&ProcessedSuggestion::new(r#"foo/baz"#, "", " ")],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default abc/foo*/ba*",
            &[&ProcessedSuggestion::new(r#"abc/foo/baz"#, "", " ")],
        );

        // Fuzzy tab completion tests
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default spaces",
            &[
                &ProcessedSuggestion::new(r#"file\ with\ spaces.txt"#, "", " "),
                &ProcessedSuggestion::new(r#"many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default $PWD/spaces",
            &[
                &ProcessedSuggestion::new(r#"$PWD/file\ with\ spaces.txt"#, "", " "),
                &ProcessedSuggestion::new(r#"$PWD/many\ spaces\ here/"#, "", ""),
            ],
        );

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default many\\ spaces\\ here/here",
            &[&ProcessedSuggestion::new(
                r#"many\ spaces\ here/and\ more\ spaces\ here.txt"#,
                "",
                " ",
            )],
        );

        // NB: Changing process cwd without changing env vars might cause problems.
        std::env::set_current_dir("/tmp/example_fs/foo/glob_stuff1").unwrap();
        bash_funcs::set_env_var("PWD", "/tmp/example_fs/foo/glob_stuff1").unwrap();

        // .* matches hidden files only. and should ignore . and ..
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default .*",
            &[&ProcessedSuggestion::new(r#".dotfile"#, "", " ")],
        );

        // ./.* matches hidden files only. and should ignore . and ..
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ./.*",
            &[&ProcessedSuggestion::new(r#"./.dotfile"#, "", " ")],
        );

        // ./* matches all non hidden
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default ./*",
            &[&ProcessedSuggestion::new(r#"./a.txt"#, "", " ")],
        );

        // * matches all non hidden
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default *",
            &[&ProcessedSuggestion::new(r#"a.txt"#, "", " ")],
        );

        // ----------------------------------------------------------------
        // Brace expansion in glob tab completion.
        // The fixture lives at /tmp/example_braces (see tab_completions.Dockerfile).
        //   foo1/{barA, barB, barC}
        //   foo2/{barA, barC}
        //   foo3/{barA, barC}
        // The pattern `$PWD/foo*{1,3}/bar*{A,C}` should expand to the
        // cartesian product of `{1,3}` x `{A,C}` and therefore match only
        // entries under foo1 and foo3, and only bar*A / bar*C — never foo2
        // and never barB.
        std::env::set_current_dir("/tmp/example_braces").unwrap();
        bash_funcs::set_env_var("PWD", "/tmp/example_braces").unwrap();

        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default $PWD/foo*{1,3}/bar*{A,C}",
            &[&ProcessedSuggestion::new(
                r#"$PWD/foo1/barA $PWD/foo1/barC $PWD/foo3/barA $PWD/foo3/barC "#,
                "",
                "",
            )],
        );

        // Single brace group combined with a glob character. This expands
        // to two patterns (`$PWD/foo1/bar*A` and `$PWD/foo1/bar*C`) which
        // together match exactly `barA` and `barC` under foo1.
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default $PWD/foo1/bar*{A,C}",
            &[&ProcessedSuggestion::new(
                r#"$PWD/foo1/barA $PWD/foo1/barC "#,
                "",
                "",
            )],
        );

        // Brace alternatives where one branch matches nothing should still
        // surface the matches from the other branch (foo2 has no barB).
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default $PWD/foo{1,2}/bar*B",
            &[&ProcessedSuggestion::new(r#"$PWD/foo1/barB"#, "", " ")],
        );

        // The same path matched by multiple brace expansions can be shown
        // multiple times. `bar*{A,A}` should produce duplicates.
        run_test_on(
            "fl_comp_util_bashdefault --fallback-to-default $PWD/foo1/bar*{A,A}",
            &[&ProcessedSuggestion::new(
                r#"$PWD/foo1/barA $PWD/foo1/barA "#,
                "",
                "",
            )],
        );

        println!("Tab completion tests FLYLINE_TEST_SUCCESS");
    }
}
