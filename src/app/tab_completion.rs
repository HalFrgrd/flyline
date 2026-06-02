use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::vec;

use crate::active_suggestions::{
    ActiveSuggestions, ActiveSuggestionsBuilder, ProcessedSuggestion, SuggestionDescription,
    UnprocessedSuggestion,
};
use crate::app::{App, ContentMode, TabCompletionHandle};
use crate::bash_funcs::{self, QuoteType};
use crate::content_utils::{self, ansi_string_to_spans};
use crate::globbing::PathPatternExpansion;
use crate::iter_first_last::FirstLast;
use crate::tab_completion_context::CompType;
use crate::text_buffer::SubString;
use crate::users;
use crate::{cli::complete_flyline_args, tab_completion_context};
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

fn run_comp_spec_completion(
    completion_context: &tab_completion_context::CompletionContext,
    initial_command_word: &str,
) -> Option<ActiveSuggestionsBuilder> {
    let poss_alias = bash_funcs::find_alias(initial_command_word);
    log::debug!(
        "Checking for alias for command word '{}': {:?}",
        initial_command_word,
        poss_alias
    );
    let alias_def = poss_alias
        .as_deref()
        .filter(|alias| !alias.is_empty())
        .unwrap_or(initial_command_word);
    let alias_expanded_completion_context = completion_context
        .with_cursor_at_end_of_wuc()
        .with_expanded_alias(alias_def);
    let alias_expanded_command_word = alias_def
        .split_whitespace()
        .next()
        .unwrap_or(alias_def)
        .to_string();
    let alias_expanded_full_command = alias_expanded_completion_context.context.as_ref();
    let alias_expanded_cursor_byte_pos =
        alias_expanded_completion_context.cursor_byte_pos_context_relative();
    let alias_expanded_word_under_cursor =
        alias_expanded_completion_context.word_under_cursor.as_ref();
    let alias_expanded_word_under_cursor_end =
        alias_expanded_completion_context.word_under_cursor_end_context_relative();

    if alias_expanded_command_word == "flyline" {
        run_flyline_compspec(alias_expanded_completion_context)
    } else {
        let poss_completions = bash_funcs::run_programmable_completions(
            alias_expanded_full_command,
            &alias_expanded_command_word,
            alias_expanded_word_under_cursor,
            alias_expanded_cursor_byte_pos,
            alias_expanded_word_under_cursor_end,
        );

        match poss_completions {
            Ok(comp_result) => {
                log::debug!(
                    "Programmable completion results for command: {}",
                    alias_expanded_full_command
                );
                log::debug!("Completions: {:#?}", comp_result);
                let flags = comp_result.flags;
                Some(ActiveSuggestionsBuilder::from_unprocessed(
                    comp_result
                        .completions
                        .into_iter()
                        .map(move |sug| UnprocessedSuggestion {
                            raw_text: sug,
                            full_path: None,
                            flags,
                            word_under_cursor: alias_expanded_word_under_cursor.to_string(),
                        }),
                ))
            }
            _ => None,
        }
    }
}

fn run_flyline_compspec(
    completion_context: tab_completion_context::CompletionContext,
) -> Option<ActiveSuggestionsBuilder> {
    let full_command = completion_context.context.as_ref();
    let cursor_byte_pos = completion_context.cursor_byte_pos_context_relative();
    let word_under_cursor = completion_context.word_under_cursor.as_ref();

    // Flyline's own subcommand/flag completions are produced by
    // clap_complete and are already escaped/finalized. Skip the
    // bash post-processing pipeline entirely and build
    // ProcssedSuggestions directly so descriptions (the help text
    // attached to each candidate) are preserved as-is.
    match complete_flyline_args(full_command, word_under_cursor, cursor_byte_pos) {
        Ok(candidates) => {
            let quote_type = bash_funcs::find_quote_type(word_under_cursor);

            let processed: Vec<ProcessedSuggestion> = candidates
                .into_iter()
                .filter_map(|c| {
                    let value = c.get_value().to_string_lossy().to_string();
                    let value = if let Some(qt) = quote_type {
                        bash_funcs::quoting_function_rust(&value, qt, true, false)
                    } else {
                        value.clone()
                    };
                    let (value, suffix) = if let Some(stripped) = value.strip_suffix("NO_SUFFIX") {
                        (stripped.to_string(), "")
                    } else {
                        (value, " ")
                    };
                    let (prefix, value) = if let Some(delim_pos) = value.find("PREFIX_DELIM") {
                        let p = value[..delim_pos].to_string();
                        let v = value[delim_pos + "PREFIX_DELIM".len()..].to_string();
                        (p, v)
                    } else {
                        (String::new(), value)
                    };

                    let description = match c.get_help() {
                        Some(h) => {
                            let ansi_help = format!("{}", h.ansi());
                            SuggestionDescription::Animation(
                                ansi_help
                                    .split('\t')
                                    .map(|s| ansi_string_to_spans(s))
                                    .collect(),
                            )
                        }
                        None => SuggestionDescription::Static(vec![]),
                    };

                    Some(
                        ProcessedSuggestion::new(&value, prefix, suffix)
                            .with_description(description),
                    )
                })
                .collect();

            Some(ActiveSuggestionsBuilder::from_processed(processed))
        }
        Err(e) => {
            log::error!("Error generating flyline completions: {}", e);
            None
        }
    }
}

/// Top-level completion entry point used by `start_tab_complete` and tests.
///
/// Calls `gen_completions_uncomitted` (which may yield a partially-processed
/// `ActiveSuggestionsBuilder`), then applies the post-processing that used
/// to live in `start_tab_complete`: drain the queue of unprocessed
/// suggestions and, when applicable, compute the longest common prefix.
///
/// Under `cfg(test)` we always force full processing (regardless of how big
/// the queue is) and always populate the common prefix, so that test
/// expectations are deterministic.
pub(crate) fn gen_completions_internal(
    completion_context: &tab_completion_context::CompletionContext,
    auto_started: bool,
) -> Option<ActiveSuggestionsBuilder> {
    let mut builder = gen_completions_uncomitted(completion_context, auto_started)?;

    let all_processed = if cfg!(test) {
        // Tests demand determinism: process everything and always compute
        // the common prefix even if `insert_common_prefix` is false.
        while !builder.try_process_all() {}
        true
    } else {
        builder.try_process_all()
    };

    if !all_processed {
        log::debug!("Not all suggestions were fully processed; skipping common prefix calculation");
    }

    if builder.insert_common_prefix && all_processed {
        builder.set_common_prefix();
    }

    Some(builder)
}

fn gen_completions_uncomitted(
    completion_context: &tab_completion_context::CompletionContext,
    auto_started: bool,
) -> Option<ActiveSuggestionsBuilder> {
    log::debug!("Completion context: {:#?}", completion_context);

    let word_under_cursor = &completion_context.word_under_cursor;

    for comp_type in &completion_context.comp_types() {
        log::debug!("Processing completion type: {:?}", comp_type);
        match comp_type {
            CompType::None => {
                log::debug!("CompType::None, skipping to next CompType");
                continue;
            }
            CompType::FirstWord => {
                log::debug!("CompType::FirstWord for: {}", word_under_cursor.as_ref());
                let completions =
                    tab_complete_first_word(word_under_cursor.as_ref(), word_under_cursor.as_ref());
                log::debug!(
                    "CompType::FirstWord found {} completions for prefix: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(completions.with_comp_type(comp_type.clone()));
                }
            }
            CompType::FuzzyFirstWord => {
                log::debug!(
                    "CompType::FuzzyFirstWord for: {}",
                    word_under_cursor.as_ref()
                );
                let completions = tab_complete_fuzzy_first_word(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::FuzzyFirstWord found {} completions for prefix: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(completions.with_comp_type(comp_type.clone()));
                }
            }
            CompType::CommandComp {
                command_word: initial_command_word,
            } => {
                // This isn't just for commands like `git`, `cargo`
                // Because we call bash_symbols::programmable_completions
                // Bash also completes env vars (`echo $HO`) and other useful completions.
                // Bash doesn't handle alias expansion well:
                // https://www.reddit.com/r/bash/comments/eqwitd/programmable_completion_on_expanded_aliases_not/
                // Since aliases are the highest priority in command word resolution,
                // If it is an alias, lets expand it here for better completion results.
                if let Some(builder) =
                    run_comp_spec_completion(completion_context, initial_command_word)
                {
                    log::debug!(
                        "CompType::CommandComp found {} completions for command word: {}",
                        builder.len(),
                        initial_command_word
                    );
                    if !builder.is_empty() {
                        return Some(builder.with_comp_type(comp_type.clone()));
                    }
                }
            }

            CompType::FuzzyCommandComp {
                command_word: initial_command_word,
            } => {
                let original_wuc = word_under_cursor.as_ref();
                log::debug!("CompType::FuzzyCommandComp for: {}", original_wuc);

                let new_wuc: String = if original_wuc.starts_with("--") {
                    original_wuc.chars().take(2).collect()
                } else if original_wuc.len() <= 1 {
                    continue;
                } else {
                    original_wuc.chars().take(1).collect()
                };

                let fuzzy_completion_context = completion_context.with_wuc_replaced(&new_wuc);

                if let Some(mut builder) =
                    run_comp_spec_completion(&fuzzy_completion_context, initial_command_word)
                {
                    let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);
                    let pattern = original_wuc.strip_prefix(&new_wuc).unwrap_or(original_wuc);

                    builder.processed = builder
                        .processed
                        .into_iter()
                        .filter_map(|sug| {
                            let match_text = &sug.s.strip_prefix(&new_wuc).unwrap_or(&sug.s);
                            content_utils::fuzzy_match_with_threshold(
                                &matcher,
                                match_text,
                                pattern,
                                content_utils::FuzzyMatchThreshold::High,
                            )
                            .inspect(|score| {
                                log::debug!("Fuzzy match score for '{}': {}", match_text, score)
                            })
                            .map(|_score| sug)
                        })
                        .collect();

                    builder.unprocessed = builder
                        .unprocessed
                        .into_iter()
                        .filter_map(|sug| {
                            let match_text = &sug
                                .match_text()
                                .strip_prefix(&new_wuc)
                                .unwrap_or(&sug.match_text());
                            content_utils::fuzzy_match_with_threshold(
                                &matcher,
                                match_text,
                                pattern,
                                content_utils::FuzzyMatchThreshold::High,
                            )
                            .inspect(|score| {
                                log::debug!("Fuzzy match score for '{}': {}", match_text, score)
                            })
                            .map(|_score| sug)
                        })
                        .collect();
                    builder = builder
                        .with_auto_accept_if_solo(false)
                        .with_insert_common_prefix(false);
                    log::debug!(
                        "CompType::FuzzyCommandComp found {} completions for pattern: {}",
                        builder.len(),
                        pattern
                    );
                    if !builder.is_empty() {
                        return Some(builder.with_comp_type(comp_type.clone()));
                    }
                }
            }

            CompType::EnvVariable => {
                log::debug!("CompType::EnvVariable for {}", word_under_cursor.as_ref());
                let matching_vars =
                    bash_funcs::get_all_variables_with_prefix(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::EnvVariable found {} completions for prefix: {}",
                    matching_vars.len(),
                    word_under_cursor.as_ref()
                );
                if !matching_vars.is_empty() {
                    return Some(
                        ActiveSuggestionsBuilder::from_processed(
                            ProcessedSuggestion::from_string_vec(matching_vars, "", " "),
                        )
                        .with_comp_type(comp_type.clone()),
                    );
                }
            }
            CompType::HostnameExpansion => {
                log::debug!(
                    "CompType::HostnameExpansion for {}",
                    word_under_cursor.as_ref()
                );
                let completions = tab_complete_hostname_expansion(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::HostnameExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(
                        ActiveSuggestionsBuilder::from_processed(completions)
                            .with_comp_type(comp_type.clone()),
                    );
                }
            }
            CompType::TildeExpansion => {
                log::debug!(
                    "CompType::TildeExpansion for {}",
                    word_under_cursor.as_ref()
                );
                let completions = tab_complete_tilde_expansion(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::TildeExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(
                        ActiveSuggestionsBuilder::from_processed(completions)
                            .with_comp_type(comp_type.clone()),
                    );
                }
            }
            CompType::GlobExpansion => {
                if auto_started {
                    log::debug!("Skipping GlobExpansion because auto_started is true");
                    continue;
                }
                log::debug!("CompType::GlobExpansion for {}", word_under_cursor.as_ref());
                let (completions, comp_res_flags) = tab_complete_glob_expansion(
                    word_under_cursor.as_ref(),
                    word_under_cursor.as_ref(),
                );

                log::debug!(
                    "CompType::GlobExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                match completions.as_slice() {
                    [] => {}
                    [single_completion] => {
                        let processed = single_completion.clone().into_processed();
                        return Some(
                            ActiveSuggestionsBuilder::from_processed([processed])
                                .with_comp_type(comp_type.clone()),
                        );
                    }
                    _ => {
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
                                acc.push_str(&sug.prefix);
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
                        return Some(
                            ActiveSuggestionsBuilder::from_processed([ProcessedSuggestion::new(
                                completions_as_string,
                                "",
                                "",
                            )])
                            .with_comp_type(comp_type.clone()),
                        );
                    }
                }
            }
            CompType::FilenameExpansion => {
                if auto_started {
                    log::debug!("Skipping FilenameExpansion because auto_started is true");
                    continue;
                }
                log::debug!(
                    "CompType::FilenameExpansion for: {}",
                    word_under_cursor.as_ref()
                );
                let (completions, _comp_res_flags) = tab_complete_glob_expansion(
                    &(completion_context.word_left_of_cursor().to_string()
                        + "*"
                        + completion_context.word_right_of_cursor()),
                    word_under_cursor.as_ref(),
                );

                log::debug!(
                    "CompType::FilenameExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(
                        ActiveSuggestionsBuilder::from_unprocessed(completions)
                            .with_insert_common_prefix(
                                completion_context.word_right_of_cursor().is_empty(),
                            )
                            .with_comp_type(comp_type.clone()),
                    );
                }
            }
            CompType::FuzzyFilenameExpansion => {
                if auto_started {
                    log::debug!("Skipping FuzzyFilenameExpansion because auto_started is true");
                    continue;
                }
                log::debug!(
                    "CompType::FuzzyFilenameExpansion for: {}",
                    word_under_cursor.as_ref()
                );
                let (completions, _comp_res_flags) =
                    tab_complete_fuzzy_filename(completion_context);

                log::debug!(
                    "CompType::FuzzyFilenameExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(
                        ActiveSuggestionsBuilder::from_unprocessed(completions)
                            .with_auto_accept_if_solo(false)
                            .with_insert_common_prefix(false)
                            .with_comp_type(comp_type.clone()),
                    );
                }
            }
        }
    }

    log::debug!("No completion types produced result");
    None
}

fn filter_out_non_executables(paths: Vec<UnprocessedSuggestion>) -> Vec<UnprocessedSuggestion> {
    paths
        .into_iter()
        .filter(|s| {
            let Some(path) = s.full_path.as_ref() else {
                return true;
            };
            if let Ok(sym_meta) = path.symlink_metadata() {
                if sym_meta.file_type().is_symlink() {
                    return true;
                }
            }
            if let Ok(meta) = path.metadata() {
                if meta.is_dir() {
                    return true;
                }
                if meta.is_file() {
                    use std::os::unix::fs::PermissionsExt;
                    return meta.permissions().mode() & 0o111 != 0;
                }
            }
            true
        })
        .collect()
}

fn tab_complete_first_word(command: &str, word_under_cursor: &str) -> ActiveSuggestionsBuilder {
    log::debug!("Generating first word completions for: '{}'", command);
    if command.is_empty() {
        return ActiveSuggestionsBuilder::new();
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        // Path to executable
        let (files, _comp_res_flags) =
            tab_complete_glob_expansion(&(command.to_string() + "*"), word_under_cursor);
        let executable_files = filter_out_non_executables(files);
        return ActiveSuggestionsBuilder::from_unprocessed(executable_files);
    }

    let mut res = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for poss_completion in bash_funcs::get_possible_command_words() {
        if poss_completion.starts_with(command) && seen.insert(poss_completion.clone()) {
            res.push(poss_completion);
        }
    }

    if res.is_empty() {
        return ActiveSuggestionsBuilder::new();
    }

    res.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    res.dedup();
    ActiveSuggestionsBuilder::from_processed(ProcessedSuggestion::from_string_vec(res, "", " "))
}

fn tab_complete_fuzzy_first_word(command: &str) -> ActiveSuggestionsBuilder {
    log::debug!("Generating fuzzy first word completions for: '{}'", command);
    if command.is_empty() {
        return ActiveSuggestionsBuilder::new();
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        let (fuzzy_files, _comp_res_flags) = tab_complete_fuzzy_filename_from_word(command);
        let executable_files = filter_out_non_executables(fuzzy_files);
        return ActiveSuggestionsBuilder::from_unprocessed(executable_files);
    }

    let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);
    let mut scored = vec![];

    let mut seen: HashSet<String> = HashSet::new();
    for poss_completion in bash_funcs::get_possible_command_words() {
        if seen.insert(poss_completion.clone())
            && let Some(score) = content_utils::fuzzy_match_with_threshold(
                &matcher,
                &poss_completion,
                command,
                content_utils::FuzzyMatchThreshold::High,
            )
        {
            scored.push((score, poss_completion));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let res = scored.into_iter().map(|(_, s)| s).collect();
    ActiveSuggestionsBuilder::from_processed(ProcessedSuggestion::from_string_vec(res, "", " "))
}

/// Core glob expansion logic that works with an already-expanded PathPatternExpansion.
/// This is the common logic used by both prefix-matching and fuzzy-filename completion paths.
///
/// `should_skip_hidden`: If true, skip files starting with `.` (unless pattern explicitly requests them).
fn tab_complete_with_expanded_pattern(
    expanded: &PathPatternExpansion,
    comp_resultflags: bash_funcs::CompletionFlags,
    wuc: &str,
    should_skip_hidden: bool,
) -> Vec<UnprocessedSuggestion> {
    let mut results = Vec::new();

    const MAX_GLOB_RESULTS: usize = 10_000;

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
                word_under_cursor: wuc.to_string(),
            });
        }
    }

    results.sort_by(|a, b| a.match_text().cmp(b.match_text()));
    results
}

fn tab_complete_glob_expansion(
    pattern: &str,
    word_under_cursor: &str,
) -> (Vec<UnprocessedSuggestion>, bash_funcs::CompletionFlags) {
    let mut comp_resultflags = bash_funcs::CompletionFlags::default();
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
    let completions =
        tab_complete_with_expanded_pattern(&expanded, comp_resultflags, word_under_cursor, true);

    (completions, comp_resultflags)
}

/// List all files in the directory implied by `word_under_cursor` and return
/// those that fuzzy-match the last path segment using the Arinae matcher.
///
/// This is the fallback when [`tab_complete_glob_expansion`] (prefix matching)
/// finds no results: e.g. typing `src/tm` won't prefix-match `src/tab_completion.rs`,
/// but the fuzzy matcher will.
fn tab_complete_fuzzy_filename_from_word(
    word_under_cursor: &str,
) -> (Vec<UnprocessedSuggestion>, bash_funcs::CompletionFlags) {
    tab_complete_fuzzy_filename_impl(word_under_cursor, 0)
}

fn tab_complete_fuzzy_filename(
    completion_context: &tab_completion_context::CompletionContext,
) -> (Vec<UnprocessedSuggestion>, bash_funcs::CompletionFlags) {
    let cursor_seg_from_right = completion_context
        .word_right_of_cursor()
        .matches('/')
        .count();
    tab_complete_fuzzy_filename_impl(
        completion_context.word_under_cursor.as_ref(),
        cursor_seg_from_right,
    )
}

fn tab_complete_fuzzy_filename_impl(
    word_under_cursor: &str,
    cursor_seg_from_right: usize,
) -> (Vec<UnprocessedSuggestion>, bash_funcs::CompletionFlags) {
    let mut comp_res_flags = bash_funcs::CompletionFlags::default();
    comp_res_flags.filename_quoting_desired = false;
    comp_res_flags.filename_completion_desired = true;
    comp_res_flags.quote_type = bash_funcs::find_quote_type(word_under_cursor);

    let dequoted_wuc = bash_funcs::dequoting_function_rust(word_under_cursor);
    let (is_absolute, segments) = split_nonempty_path_segments(&dequoted_wuc);
    if segments.is_empty() {
        return (vec![], comp_res_flags);
    }

    let cursor_seg_idx = segments
        .len()
        .saturating_sub(cursor_seg_from_right.saturating_add(1));
    let (prefix_segments, fuzzy_segments) = segments.split_at(cursor_seg_idx);
    if fuzzy_segments.is_empty() {
        return (vec![], comp_res_flags);
    }

    let base_input = path_from_segments(is_absolute, prefix_segments);
    let expanded_base = PathBuf::from(bash_funcs::fully_expand_path(if base_input.is_empty() {
        "."
    } else {
        &base_input
    }));
    let raw_prefix = path_prefix_for_output(is_absolute, prefix_segments);

    let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);
    let mut scored = fuzzy_glob_recursive(&expanded_base, fuzzy_segments, &matcher);
    if scored.is_empty() {
        return (vec![], comp_res_flags);
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.dedup_by(|a, b| a.1 == b.1);

    let completions = scored
        .into_iter()
        .map(|(_score, matched_segments, final_path)| {
            let mut raw_text = raw_prefix.clone();
            raw_text.push_str(&matched_segments.join("/"));

            UnprocessedSuggestion {
                raw_text,
                full_path: Some(final_path),
                flags: comp_res_flags,
                word_under_cursor: String::new(),
            }
        })
        .collect();

    (completions, comp_res_flags)
}

fn split_nonempty_path_segments(path: &str) -> (bool, Vec<String>) {
    let is_absolute = path.starts_with('/');
    let segments = path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .map(ToString::to_string)
        .collect();
    (is_absolute, segments)
}

fn path_from_segments(is_absolute: bool, segments: &[String]) -> String {
    if segments.is_empty() {
        if is_absolute {
            "/".to_string()
        } else {
            String::new()
        }
    } else {
        let mut out = String::new();
        if is_absolute {
            out.push('/');
        }
        out.push_str(&segments.join("/"));
        out
    }
}

fn path_prefix_for_output(is_absolute: bool, segments: &[String]) -> String {
    let mut out = path_from_segments(is_absolute, segments);
    if !out.is_empty() && !out.ends_with('/') {
        out.push('/');
    }
    out
}

fn fuzzy_glob_recursive(
    base_dir: &Path,
    remaining_segments: &[String],
    matcher: &ArinaeMatcher,
) -> Vec<(i64, Vec<String>, PathBuf)> {
    if remaining_segments.is_empty() {
        return vec![(0, vec![], base_dir.to_path_buf())];
    }

    let mut out = Vec::new();
    let pattern = &remaining_segments[0];
    let is_last = remaining_segments.len() == 1;

    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(score) = content_utils::fuzzy_match_with_threshold(
            matcher,
            &name,
            pattern,
            content_utils::FuzzyMatchThreshold::Medium,
        ) else {
            continue;
        };

        let path = entry.path();
        let file_type = entry.file_type().ok();

        if is_last {
            out.push((score, vec![name], path));
            continue;
        }

        if !file_type.is_some_and(|ft| ft.is_dir()) {
            continue;
        }

        for (child_score, child_segments, final_path) in
            fuzzy_glob_recursive(&path, &remaining_segments[1..], matcher)
        {
            let mut segments = Vec::with_capacity(1 + child_segments.len());
            segments.push(name.clone());
            segments.extend(child_segments);
            out.push((score + child_score, segments, final_path));
        }
    }

    out
}

fn tab_complete_hostname_expansion(pattern: &str) -> Vec<ProcessedSuggestion> {
    let at_idx = if let Some(idx) = pattern.rfind('@') {
        idx
    } else {
        return vec![];
    };

    let user_pattern = &pattern[at_idx + 1..];
    let prefix = &pattern[..=at_idx];

    let mut suggestions = Vec::new();

    for hostname in crate::hostnames::get_all_hostnames() {
        if hostname.starts_with(user_pattern) {
            suggestions.push(ProcessedSuggestion::new(
                format!("{}{}", prefix, hostname),
                "",
                "",
            ));
        }
    }

    suggestions.sort_by(|a, b| a.s.cmp(&b.s));
    suggestions.dedup_by(|a, b| a.s == b.s);
    suggestions
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

/// Outcome of applying tab-completion results directly to a [`TextBuffer`].
///
/// This is the buffer-mutation half of `finish_tab_complete` factored out so
/// it can be exercised from unit tests without constructing a full `App`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum TabCompleteBufferOutcome {
    /// We auto-accepted a single suggestion; the caller (the App) should
    /// switch back to `ContentMode::Normal` and discard the builder.
    SoloAccepted,
    /// More than one suggestion (or fuzzy-style completion). The caller
    /// should hand the builder over to `ActiveSuggestions` for display.
    /// `final_wuc` is the new word-under-cursor `SubString` reflecting any
    /// common-prefix insertion that was applied to the buffer.
    Pending { final_wuc: SubString },
}

/// Buffer-only half of finishing a tab-completion. Mutates `buffer` in place
/// (auto-accept of a solo suggestion, or insertion of the longest common
/// prefix) and reports back what the caller should do next.
pub(crate) fn apply_tab_complete_to_buffer(
    buffer: &mut crate::text_buffer::TextBuffer,
    builder: &ActiveSuggestionsBuilder,
    wuc_substring: &SubString,
) -> TabCompleteBufferOutcome {
    let mut processed_builder = None;
    if builder.len() == 1 && builder.auto_accept_if_solo {
        let mut builder = builder.clone();
        builder.process_all_blocking();
        processed_builder = Some(builder);
    }

    if let Some(suggestion) = processed_builder
        .as_ref()
        .and_then(|builder| builder.processed.first())
    {
        log::info!(
            "Auto-accepting solo suggestion: '{:?}' for word under cursor '{:?}'",
            suggestion,
            wuc_substring
        );
        buffer
            .replace_word_under_cursor(&suggestion.formatted(), wuc_substring)
            .ok();
        return TabCompleteBufferOutcome::SoloAccepted;
    }

    if builder.is_empty() {
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
        match buffer.replace_word_under_cursor(common_prefix, wuc_substring) {
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

    TabCompleteBufferOutcome::Pending { final_wuc }
}

impl App<'_> {
    /// Apply the results of tab completion generation (Phase 2 & 3: common
    /// prefix insertion and handing suggestions to the UI).
    pub fn finish_tab_complete(
        &mut self,
        builder: ActiveSuggestionsBuilder,
        wuc_substring: SubString,
        load_time: std::time::Duration,
        auto_started: bool,
    ) {
        if auto_started {
            if builder.is_empty() {
                self.content_mode = ContentMode::Normal;
                return;
            }
            let total_len = builder.processed.len() + builder.unprocessed.len();
            if total_len == 1 {
                let matches_exact = if let Some(processed) = builder.processed.first() {
                    processed.s == wuc_substring.s
                } else if let Some(unprocessed) = builder.unprocessed.front() {
                    unprocessed.match_text() == wuc_substring.s
                } else {
                    false
                };
                if matches_exact {
                    self.content_mode = ContentMode::Normal;
                    return;
                }
            }
            let suggestions =
                ActiveSuggestions::new(builder, wuc_substring, load_time, auto_started);
            self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
        } else {
            let outcome = apply_tab_complete_to_buffer(&mut self.buffer, &builder, &wuc_substring);
            match outcome {
                TabCompleteBufferOutcome::SoloAccepted => {
                    self.content_mode = ContentMode::Normal;
                }
                TabCompleteBufferOutcome::Pending { final_wuc } => {
                    let suggestions =
                        ActiveSuggestions::new(builder, final_wuc, load_time, auto_started);
                    self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
                }
            }
        }
    }

    pub fn start_tab_complete(&mut self, auto_started: bool) {
        // Phase 1: compute the completion context and generate suggestions.
        // We store word_under_cursor as an owned SubString so we can use it
        // after the immutable-borrow block ends.

        let completion_context = tab_completion_context::get_completion_context(
            self.buffer.buffer(),
            self.buffer.cursor_byte_pos(),
        );

        let wuc_substring = completion_context.word_under_cursor.clone();

        let (tx, rx) =
            std::sync::mpsc::channel::<Option<(ActiveSuggestionsBuilder, std::time::Duration)>>();

        let completion_context_owned = completion_context.into_owned();

        let start_time = std::time::Instant::now();

        let thread_handle = std::thread::spawn(move || {
            let thread_start = std::time::Instant::now();
            let result = gen_completions_internal(&completion_context_owned, auto_started);
            let elapsed = thread_start.elapsed();
            if result.is_none() {
                log::debug!(
                    "No suggestions generated for completion context: {:?}",
                    completion_context_owned
                );
            }
            if let Err(e) = tx.send(result.map(|r| (r, elapsed))) {
                log::warn!(
                    "Tab completion: failed to send result (receiver dropped): {:?}",
                    e
                );
            }
        });

        // Block for up to 100ms waiting for the thread to finish.
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Some((builder, elapsed))) => {
                self.finish_tab_complete(builder, wuc_substring, elapsed, auto_started);
            }
            Ok(None) => {
                // No suggestions generated.
                self.finish_tab_complete(
                    ActiveSuggestionsBuilder::new(),
                    wuc_substring,
                    start_time.elapsed(),
                    auto_started,
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
                    auto_started,
                };
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("Tab completion thread disconnected unexpectedly");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Library-test versions of the docker-based tab completion tests.
//
// These tests exercise `gen_completions_internal` and
// `apply_tab_complete_to_buffer` directly against a `TextBuffer` instead of
// constructing a full `App`. Tests that mutate process-wide state (env vars,
// current working directory) run under `rusty_fork_test!` so each test gets
// its own fresh process.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tab_completion_tests {
    use super::*;
    use crate::active_suggestions::{FilteredItem, ProcessedSuggestion, UnprocessedSuggestion};
    use crate::tab_completion_context::{CompletionContext, get_completion_context};
    use crate::text_buffer::TextBuffer;
    use rusty_fork::rusty_fork_test;

    const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

    /// Locate a test fixture directory by trying multiple paths:
    /// 1. Relative path from current working directory (works in most test runners)
    /// 2. Path relative to CARGO_MANIFEST_DIR (works in some Docker builds)
    fn find_test_fixture_dir(subdir: &str) -> String {
        let relative_path = format!("tests/{}", subdir);
        if std::path::Path::new(&relative_path).exists() {
            return relative_path;
        }

        let manifest_path = format!("{}/tests/{}", MANIFEST_DIR, subdir);
        if std::path::Path::new(&manifest_path).exists() {
            return manifest_path;
        }

        panic!(
            "Could not locate test fixture directory '{}'. Tried:\n  - {}\n  - {}",
            subdir, relative_path, manifest_path
        );
    }

    /// Run completion against `command` (cursor placed at the end of the
    /// string), drain anything still queued, then return the processed
    /// suggestions sorted by `s` for stable comparison.
    fn run_completion(command: &str) -> Vec<ProcessedSuggestion> {
        let buffer = TextBuffer::new(command);
        run_completion_from_buffer(&buffer)
    }

    fn get_builder(
        command: &str,
    ) -> Option<(ActiveSuggestionsBuilder, CompletionContext<'static>)> {
        let buffer = TextBuffer::new(command);
        get_builder_from_buffer(&buffer)
    }

    fn get_builder_from_buffer(
        buffer: &TextBuffer,
    ) -> Option<(ActiveSuggestionsBuilder, CompletionContext<'static>)> {
        crate::logging::init_for_tests_once();
        let comp_context = get_completion_context(buffer.buffer(), buffer.cursor_byte_pos());
        let Some(builder) = gen_completions_internal(&comp_context, false) else {
            return None;
        };
        Some((builder, comp_context.into_owned()))
    }

    fn run_completion_from_buffer(buffer: &TextBuffer) -> Vec<ProcessedSuggestion> {
        crate::logging::init_for_tests_once();

        let Some((builder, _)) = get_builder_from_buffer(buffer) else {
            return Vec::new();
        };
        let mut suggestions: Vec<ProcessedSuggestion> = builder.processed;
        suggestions.sort_by(|a, b| a.s.cmp(&b.s));
        suggestions
    }

    fn run_to_active_suggestions(buffer: &mut TextBuffer) -> ActiveSuggestions {
        crate::logging::init_for_tests_once();

        let (builder, comp_context) = get_builder_from_buffer(buffer).unwrap();
        let outcome =
            apply_tab_complete_to_buffer(buffer, &builder, &comp_context.word_under_cursor);
        let final_wuc = if let TabCompleteBufferOutcome::Pending { final_wuc } = outcome {
            final_wuc
        } else {
            panic!("Expected pending outcome with suggestions");
        };
        ActiveSuggestions::new(builder, final_wuc, std::time::Duration::from_secs(0), false)
    }

    fn assert_completions(command: &str, expected: &[ProcessedSuggestion]) {
        let actual = run_completion(command);
        assert_processed(&actual, expected);
    }

    fn assert_processed(actual: &[ProcessedSuggestion], expected: &[ProcessedSuggestion]) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "completion count mismatch: got {:?}, expected {:?}",
            actual,
            expected
        );
        // Dont check the description since mtime is hard to test
        for (got, want) in actual.iter().zip(expected.iter()) {
            assert_eq!(
                (&got.prefix, &got.s, &got.suffix),
                (&want.prefix, &want.s, &want.suffix),
                "got {:?}, expected {:?}",
                got,
                want
            );
        }
    }

    fn cd_to_example_fs() {
        let dir = find_test_fixture_dir("example_fs");
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
        // No need to set the `PWD` env var: the `#[cfg(test)]` bash_funcs
        // (in particular `get_envvar_value` / `expand_filename`) source
        // `$PWD` from the process's current working directory via
        // `bash_funcs::test_fixtures::test_env_vars`.
    }

    fn cd_to_example_braces_fs() {
        let dir = find_test_fixture_dir("example_braces_fs");
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
    }

    fn cd_to_example_glob_fs() {
        let dir = find_test_fixture_dir("example_glob_fs");
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
    }

    fn cd_to_example_fuzzy_glob_fs() {
        let dir = find_test_fixture_dir("example_fuzzy_glob_fs");
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
    }

    fn cd_to_example_long_filenames_fs() {
        let dir = find_test_fixture_dir("example_long_filenames_fs");
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
    }

    rusty_fork_test! {
        // ------- dummy git completion (clap-based, no bash symbols) -------

        #[test]
        fn hostname_completion() {
            let actual = run_completion("ssh us@localho");
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            assert_eq!(names, vec!["us@localhost"]);
        }

        #[test]
        fn git_top_level_subcommand_a_completes_to_add() {
            cd_to_example_fs();
            let actual = run_completion("git a");
            // The dummy git CLI only knows about add/commit/diff/status.
            // "a" only matches "add" so we expect exactly one candidate.
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            assert!(names.contains(&"add"), "expected `add` in {:?}", names);
        }

        #[test]
        fn git_top_level_no_prefix_lists_subcommands() {
            cd_to_example_fs();
            let actual = run_completion("git ");
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            for sub in ["add", "commit", "diff", "status"] {
                assert!(names.contains(&sub), "expected `{sub}` in {:?}", names);
            }
        }

        #[test]
        fn git_commit_dashdash_lists_long_flags() {
            cd_to_example_fs();
            let actual = run_completion("git commit --");
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            for flag in ["--message", "--amend", "--all", "--no-verify"] {
                assert!(names.contains(&flag), "expected {flag} in {:?}", names);
            }
        }

        #[test]
        fn git_diff_dashdash_lists_long_flags() {
            cd_to_example_fs();
            let actual = run_completion("git diff --");
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            for flag in ["--staged", "--stat", "--name-only", "--color"] {
                assert!(names.contains(&flag), "expected {flag} in {:?}", names);
            }
        }

        #[test]
        fn git_diff_dashdash_lists_long_flags_mid_word() {
            cd_to_example_fs();
            let buffer = TextBuffer::new_with_cursor("git diff --st█ag");

            // It doesnt matter where the cursor is because I always move it to the end
            // This gives best results since it allows the FuzzyCommandComp and Filname (that uses mid word information)
            // to run.

            let actual = run_completion_from_buffer(&buffer);
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            for flag in ["--staged"] {
                assert!(names.contains(&flag), "expected {flag} in {:?}", names);
            }

            // If we didnt move the cursor to the end,
            // we would get the same results as this one:
            let buffer = TextBuffer::new_with_cursor("git diff --st█");
            let actual = run_completion_from_buffer(&buffer);
            let names: Vec<&str> = actual.iter().map(|s| s.s.as_str()).collect();
            for flag in ["--staged", "--stat"] {
                assert!(names.contains(&flag), "expected {flag} in {:?}", names);
            }
        }

        // ------- dummy git completion fuzzy matching
        /// This tests the [crate::CompType::FuzzyCommandComp] branch where we re-run the
        #[test]
        fn git_commit_fuzzy_command_comp() {
            cd_to_example_fs();
            let builder = get_builder("git cmomit").unwrap().0; // Typo of commit
            assert_eq!(builder.comp_type, CompType::FuzzyCommandComp { command_word: "git".to_string() });
            let names: Vec<&str> = builder.processed.iter().map(|s| s.s.as_str()).collect();
            for flag in ["commit"] {
                assert!(names.contains(&flag), "expected {flag} in {:?}", names);
            }
        }

        #[test]
        fn git_commit_fuzzy_command_comp_fallback_if_not_found() {
            cd_to_example_fs();
            let builder = get_builder("git symlinktfoo").unwrap().0; // This one should fall back to filenames
            assert_eq!(builder.comp_type, CompType::FuzzyFilenameExpansion);
            assert_eq!(builder.len(), 1);
            assert_eq!(builder.processed[0].s, "sym_link_to_foo/");
        }

        #[test]
        fn docker_completions_with_inline_descriptions() {
            cd_to_example_fs();
            let actual = run_completion("docker p");

            assert_eq!(actual.len(), 2);

            let port_sug = actual.iter().find(|s| s.s == "port").unwrap();
            let ps_sug = actual.iter().find(|s| s.s == "ps").unwrap();

            assert_eq!(port_sug.s, "port");
            assert_eq!(ps_sug.s, "ps");

            if let SuggestionDescription::Animation(ref frames) = port_sug.description {
                assert_eq!(frames.len(), 1);
                let text: String = frames[0].iter().map(|span| span.content.as_ref()).collect();
                assert_eq!(text, "List port mappings or a specific mapping for the container");
            } else {
                panic!("Expected Animation description for port, got {:?}", port_sug.description);
            }

            if let SuggestionDescription::Animation(ref frames) = ps_sug.description {
                assert_eq!(frames.len(), 1);
                let text: String = frames[0].iter().map(|span| span.content.as_ref()).collect();
                assert_eq!(text, "List containers");
            } else {
                panic!("Expected Animation description for ps, got {:?}", ps_sug.description);
            }
        }

        // ------- alias expansion (find_alias / get_all_aliases) ----------

        #[test]
        fn alias_gd_dashstag_expands_to_dashstaged() {
            // `gd` is aliased to `git diff` (see bash_funcs::test_fixtures
            // test_aliases). After alias expansion, completing `--stag`
            // should yield exactly `--staged`, and because it's a solo
            // suggestion the buffer should auto-accept it.
            cd_to_example_fs();
            let mut buffer = TextBuffer::new("gd --stag");
            let comp_context =
                get_completion_context(buffer.buffer(), buffer.cursor_byte_pos());
            let wuc = comp_context.word_under_cursor.clone();
            let builder = gen_completions_internal(&comp_context, false).expect("some completions");
            assert_eq!(builder.comp_type, CompType::CommandComp { command_word: "gd".to_string() });
            assert_eq!(builder.len(), 1, "expected solo suggestion, got {:?}", builder.processed);
            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &wuc);
            assert!(matches!(outcome, TabCompleteBufferOutcome::SoloAccepted));
            assert_eq!(buffer.buffer(), "gd --staged ");
        }

        // ------- filename completion against tests/example_fs ------------

        #[test]
        fn filename_completion_in_example_fs() {
            cd_to_example_fs();
            // Use a non-git command so run_programmable_completions returns
            // nothing and the FilenameExpansion branch handles the word.
            assert_completions(
                "mycmd ./",
                &[
                    ProcessedSuggestion::new("abc/", "./", ""),
                    ProcessedSuggestion::new("bar.txt", "./", " "),
                    ProcessedSuggestion::new(r"file\ with\ spaces.txt", "./", " "),
                    ProcessedSuggestion::new("foo/", "./", ""),
                    ProcessedSuggestion::new(r"many\ spaces\ here/", "./", ""),
                    ProcessedSuggestion::new("sym_link_to_foo/", "./", ""),
                ],
            );
        }

        #[test]
        fn glob_expansion_with_glob_chars_in_dir_components() {
            cd_to_example_fs();
            assert_completions(
                "mycmd foo*/ba*",
                &[ProcessedSuggestion::new("foo/baz", "", " ")],
            );
        }

        #[test]
        fn glob_dollar_pwd_expansion() {
            cd_to_example_fs();
            assert_completions(
                "mycmd $PWD/foo*/ba*",
                &[ProcessedSuggestion::new("$PWD/foo/baz", "", " ")],
            );
        }

        #[test]
        fn brace_expansion_combined_with_glob() {
            cd_to_example_braces_fs();
            assert_completions(
                "mycmd $PWD/foo*{1,3}/bar*{A,C}",
                &[ProcessedSuggestion::new(
                    "$PWD/foo1/barA $PWD/foo1/barC $PWD/foo3/barA $PWD/foo3/barC ",
                    "",
                    "",
                )],
            );
        }

        #[test]
        fn glob_expansion_keeps_parent_path_for_each_match() {
            cd_to_example_fs();
            assert_completions(
                "echo foo/*bar*",
                &[ProcessedSuggestion::new(
                    "foo/abcbardef foo/ghibarjkl ",
                    "",
                    "",
                )],
            );
        }


        #[test]
        fn globbing_test_1() {
            cd_to_example_glob_fs();
            assert_completions(
                "mycmd bar*",
                &[ProcessedSuggestion::new(
                    "bar1 bar2 bar3 ",
                    "",
                    "",
                )],
            );
        }

        #[test]
        fn fuzzy_globbing_recurses_across_path_segments() {
            cd_to_example_fuzzy_glob_fs();

            let buffer = TextBuffer::new_with_cursor("mycmd ./tr█e/lefa/apel");

            let builder = get_builder_from_buffer(&buffer).unwrap().0;
            assert_eq!(builder.comp_type, CompType::FuzzyFilenameExpansion);

            let names: Vec<&str> = builder.processed.iter().map(|s| s.s.as_str()).collect();
            assert!(names.contains(&"./tree/leaf/apple.txt"));
            assert!(names.contains(&"./three/leaf/apple.log"));
        }


        #[test]
        fn mid_word_completion() {
            cd_to_example_fs();
            let mut buffer = TextBuffer::new_with_cursor("mycmd ./abc/f█/baz");

            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();
            assert_eq!(builder.comp_type, CompType::FilenameExpansion);
            assert_processed(
                &builder.processed,
                &[ProcessedSuggestion::new(
                    "./abc/foo/baz",
                    "",
                    " ",
                )],
            );

            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            assert!(matches!(outcome, TabCompleteBufferOutcome::SoloAccepted));
            assert_eq!(buffer.buffer(), "mycmd ./abc/foo/baz ");
        }

        #[test]
        fn mid_word_completion_multiple() {
            cd_to_example_braces_fs();
            let mut buffer = TextBuffer::new_with_cursor("mycmd ./fo█/barA");

            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();
            assert_eq!(builder.comp_type, CompType::FilenameExpansion);
            assert_processed(
                &builder.processed,
                &[ProcessedSuggestion::new(
                    "./foo1/barA",
                    "",
                    " ",
                ),ProcessedSuggestion::new(
                    "./foo2/barA",
                    "",
                    " ",
                ),ProcessedSuggestion::new(
                    "./foo3/barA",
                    "",
                    " ",
                )],
            );

            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            log::info!("Outcome of applying tab complete: {:?}", &outcome);
            assert!(matches!(outcome, TabCompleteBufferOutcome::Pending { ref final_wuc } if final_wuc.as_ref() == "./fo/barA"));
            assert_eq!(buffer.buffer(), "mycmd ./fo/barA");
        }

        #[test]
        fn mid_word_completion_naive_bash_default() {
            cd_to_example_fs();
            // Cat is setup so that run_programmable_completions in test fixtures
            // returns files matching the lhs of

            // We move the cursor to the end so this acts like "./abc/foo/ba█"
            // Which a naive glob will complete
            let mut buffer = TextBuffer::new_with_cursor("cat ./abc/foo█/ba");

            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();
            assert_eq!(builder.comp_type, CompType::CommandComp { command_word: "cat".to_string() });
            assert_processed(
                &builder.processed,
                &[ProcessedSuggestion::new(
                    "./abc/foo/baz",
                    "",
                    " ",
                )],
            );
            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            assert!(matches!(outcome, TabCompleteBufferOutcome::SoloAccepted));
            assert_eq!(buffer.buffer(), "cat ./abc/foo/baz ");


            // But now since fo folder doesnt exit (only 'foo' does)
            // command comp should fail we fall back to fuzzy filename
            let mut buffer = TextBuffer::new_with_cursor("cat ./abc/fo█/ba");

            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();
            assert_eq!(builder.comp_type, CompType::FuzzyFilenameExpansion);
            assert_processed(
                &builder.processed,
                &[ProcessedSuggestion::new(
                    "./abc/foo/baz",
                    "",
                    " ",
                )],
            );
            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            assert!(matches!(outcome, TabCompleteBufferOutcome::Pending { ref final_wuc } if final_wuc.as_ref() == "./abc/fo/ba"));
            assert_eq!(buffer.buffer(), "cat ./abc/fo/ba");
        }



        // ------- finish_tab_complete (auto-accept solo) ------------------

        #[test]
        fn finish_tab_complete_auto_accepts_solo_suggestion() {
            cd_to_example_fs();
            let mut buffer = TextBuffer::new("mycmd bar.tx");
            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();

            assert_eq!(builder.len(), 1, "expected exactly one suggestion");
            assert_eq!(builder.comp_type, CompType::FilenameExpansion);

            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            assert!(matches!(outcome, TabCompleteBufferOutcome::SoloAccepted));
            assert_eq!(buffer.buffer(), "mycmd bar.txt ");
        }

        #[test]
        fn finish_tab_complete_auto_accepts_solo_unprocessed_suggestion() {
            let mut buffer = TextBuffer::new("mycmd bar.tx");
            let wuc = get_completion_context(buffer.buffer(), buffer.cursor_byte_pos()).word_under_cursor;
            let builder = ActiveSuggestionsBuilder::from_unprocessed([UnprocessedSuggestion {
                raw_text: "bar.txt".to_string(),
                full_path: None,
                flags: crate::bash_funcs::CompletionFlags::default(),
                word_under_cursor: "bar.tx".to_string(),
            }]);

            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &wuc);

            assert!(matches!(outcome, TabCompleteBufferOutcome::SoloAccepted));
            assert_eq!(buffer.buffer(), "mycmd bar.txt ");
        }

        // ------- finish_tab_complete (common prefix insertion) -----------

        #[test]
        fn finish_tab_complete_inserts_common_prefix() {
            cd_to_example_braces_fs();
            // foo1, foo2 and foo3 all share the prefix "foo".
            let mut buffer = TextBuffer::new("mycmd f");
            let (builder, comp_context) = get_builder_from_buffer(&buffer).unwrap();
            assert!(builder.len() >= 2, "expected multiple suggestions, got {}", builder.len());
            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &comp_context.word_under_cursor);
            assert!(matches!(outcome, TabCompleteBufferOutcome::Pending { .. }));
            assert_eq!(buffer.buffer(), "mycmd foo");
        }

        // ------- fuzzy matching with long filenames -----------

        #[test]
        fn fuzzy_matching_with_long_filenames() {
            cd_to_example_long_filenames_fs();

            // Arinae fuzzy matcher stops working at a certain length 64 chars.
            // So below that, we can expect fuzzy matching to work.
            let mut buffer = TextBuffer::new_with_cursor("mycmd ./len_61_plus_3/█");
            let active_suggestions = run_to_active_suggestions(&mut buffer);
            assert_eq!(buffer.buffer(), "mycmd ./len_61_plus_3/abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_a");
            assert_processed(
                &active_suggestions.processed_suggestions,
                &[
                    ProcessedSuggestion::new(
                        "abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_aBAR",
                        "./len_61_plus_3/",
                        " ",
                    ),
                    ProcessedSuggestion::new(
                        "abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_aFOO",
                        "./len_61_plus_3/",
                        " ",
                    ),
                ],
            );
            assert_eq!(active_suggestions.filtered_suggestions, vec![
                FilteredItem{
                    suggestion_idx: 0,
                    score: 2006,
                    matching_indices: (0..=60).collect(),
                },
                FilteredItem{
                    suggestion_idx: 1,
                    score: 2006,
                    matching_indices: (0..=60).collect(),
                }
            ]);

            // But above that length, fuzzy filtering in active suggestions should just return dummy scores
            let mut buffer = TextBuffer::new_with_cursor("mycmd ./len_65_plus_3/█");
            let active_suggestions = run_to_active_suggestions(&mut buffer);
            assert_eq!(buffer.buffer(), "mycmd ./len_65_plus_3/abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_");
            assert_processed(
                &active_suggestions.processed_suggestions,
                &[
                    ProcessedSuggestion::new(
                        "abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_BAR",
                        "./len_65_plus_3/",
                        " ",
                    ),
                    ProcessedSuggestion::new(
                        "abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd_FOO",
                        "./len_65_plus_3/",
                        " ",
                    ),
                ],
            );
            assert_eq!(active_suggestions.filtered_suggestions, vec![
                FilteredItem{
                    suggestion_idx: 0,
                    score: 0,
                    matching_indices: vec![],
                },
                FilteredItem{
                    suggestion_idx: 1,
                    score: 0,
                    matching_indices: vec![],
                }
            ]);

        }
    }
}
