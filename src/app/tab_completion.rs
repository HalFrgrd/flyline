use std::collections::HashSet;
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

/// Result of running command-style completion (programmable bash
/// completion or flyline's own clap-based completer) for a single
/// completion type.
enum CompSpecCompletionResult {
    /// We have suggestions to return.
    Found(ActiveSuggestionsBuilder),
    /// No suggestions, but bash returned flags worth propagating to
    /// subsequent completion types (e.g. quote type).
    NoneWithFlags(bash_funcs::CompletionFlags),
    /// No suggestions and nothing to propagate.
    None,
}

fn run_comp_spec_completion(
    completion_context: &tab_completion_context::CompletionContext,
    initial_command_word: &str,
) -> CompSpecCompletionResult {
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
    let alias_expanded_completion_context = completion_context.with_expanded_alias(alias_def);
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
        // Flyline's own subcommand/flag completions are produced by
        // clap_complete and are already escaped/finalized. Skip the
        // bash post-processing pipeline entirely and build
        // ProcssedSuggestions directly so descriptions (the help text
        // attached to each candidate) are preserved as-is.
        match complete_flyline_args(
            alias_expanded_full_command,
            alias_expanded_word_under_cursor,
            alias_expanded_cursor_byte_pos,
        ) {
            Ok(candidates) if !candidates.is_empty() => {
                let quote_type = bash_funcs::find_quote_type(alias_expanded_word_under_cursor);

                let processed: Vec<ProcessedSuggestion> = candidates
                    .into_iter()
                    .filter_map(|c| {
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

                if processed.is_empty() {
                    return CompSpecCompletionResult::None;
                }

                let mut builder = ActiveSuggestionsBuilder::new();
                builder.extend_processed(processed);
                CompSpecCompletionResult::Found(builder)
            }
            Ok(_) => {
                log::debug!(
                    "No flyline completions found for command '{}'",
                    alias_expanded_full_command
                );
                CompSpecCompletionResult::None
            }
            Err(e) => {
                log::error!("Error generating flyline completions: {}", e);
                CompSpecCompletionResult::None
            }
        }
    } else {
        let poss_completions = bash_funcs::run_programmable_completions(
            alias_expanded_full_command,
            &alias_expanded_command_word,
            alias_expanded_word_under_cursor,
            alias_expanded_cursor_byte_pos,
            alias_expanded_word_under_cursor_end,
        );

        match poss_completions {
            Ok(comp_result) if !comp_result.completions.is_empty() => {
                log::debug!(
                    "Programmable completion results for command: {}",
                    alias_expanded_full_command
                );
                log::debug!("Completions: {:#?}", comp_result);
                let flags = comp_result.flags;
                let mut builder = ActiveSuggestionsBuilder::new();
                builder.extend_unprocessed(comp_result.completions.into_iter().map(move |sug| {
                    UnprocessedSuggestion {
                        raw_text: sug,
                        full_path: None,
                        flags,
                        word_under_cursor: alias_expanded_word_under_cursor.to_string(),
                    }
                }));
                CompSpecCompletionResult::Found(builder)
            }
            Ok(comp_result) => CompSpecCompletionResult::NoneWithFlags(comp_result.flags),
            _ => CompSpecCompletionResult::None,
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
) -> Option<ActiveSuggestionsBuilder> {
    let mut builder = gen_completions_uncomitted(completion_context)?;

    let all_processed = builder.try_process_all();
    if !all_processed {
        log::debug!("Not all suggestions were fully processed; skipping common prefix calculation");
    }

    if cfg!(test) {
        // Tests demand determinism: process everything and always compute
        // the common prefix even if `auto_accept_if_solo` is false.
        while !builder.try_process_all() {}
        builder.set_common_prefix();
    } else if builder.auto_accept_if_solo && all_processed {
        builder.set_common_prefix();
    }

    Some(builder)
}

fn gen_completions_uncomitted(
    completion_context: &tab_completion_context::CompletionContext,
) -> Option<ActiveSuggestionsBuilder> {
    log::debug!("Completion context: {:#?}", completion_context);

    let word_under_cursor = &completion_context.word_under_cursor;

    let mut comp_res_flags = bash_funcs::CompletionFlags::default();

    for comp_type in &completion_context.comp_types {
        log::debug!("Processing completion type: {:?}", comp_type);
        match comp_type {
            tab_completion_context::CompType::FirstWord => {
                log::debug!("CompType::FirstWord for: {}", word_under_cursor.as_ref());
                let completions = tab_complete_first_word(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::FirstWord found {} completions for prefix: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    return Some(completions);
                }
            }
            tab_completion_context::CompType::FuzzyFirstWord => {
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
                match run_comp_spec_completion(completion_context, initial_command_word) {
                    CompSpecCompletionResult::Found(builder) => {
                        return Some(builder);
                    }
                    CompSpecCompletionResult::NoneWithFlags(flags) => {
                        // I am not checking if the user wants more completions (i.e. readline_default_fallback_desired)
                        // Always try to produce secondary completions
                        comp_res_flags = flags;
                    }
                    CompSpecCompletionResult::None => {}
                }
            }

            tab_completion_context::CompType::FuzzyCommandComp {
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

                match run_comp_spec_completion(&fuzzy_completion_context, initial_command_word) {
                    CompSpecCompletionResult::Found(mut builder) => {
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
                        builder = builder.with_auto_accept_if_solo(false);
                        return Some(builder);
                    }
                    CompSpecCompletionResult::NoneWithFlags(flags) => {
                        comp_res_flags = flags;
                    }
                    CompSpecCompletionResult::None => {}
                }
            }

            tab_completion_context::CompType::EnvVariable => {
                log::debug!("CompType::EnvVariable for {}", word_under_cursor.as_ref());
                let matching_vars =
                    bash_funcs::get_all_variables_with_prefix(word_under_cursor.as_ref());
                log::debug!(
                    "CompType::EnvVariable found {} completions for prefix: {}",
                    matching_vars.len(),
                    word_under_cursor.as_ref()
                );
                if !matching_vars.is_empty() {
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
                    let mut builder = ActiveSuggestionsBuilder::new();
                    builder.extend_processed(completions);
                    return Some(builder);
                }
            }
            tab_completion_context::CompType::GlobExpansion => {
                log::debug!("CompType::GlobExpansion for {}", word_under_cursor.as_ref());
                let completions =
                    tab_complete_glob_expansion(word_under_cursor.as_ref(), comp_res_flags);

                log::debug!(
                    "CompType::GlobExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                match completions.as_slice() {
                    [] => {}
                    [single_completion] => {
                        let processed = single_completion.clone().into_processed();
                        let mut builder = ActiveSuggestionsBuilder::new();
                        builder.push_processed(processed);
                        return Some(builder);
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
                log::debug!(
                    "CompType::FilenameExpansion for: {}",
                    word_under_cursor.as_ref()
                );
                let completions = tab_complete_glob_expansion(
                    &(word_under_cursor.as_ref().to_string() + "*"),
                    comp_res_flags,
                );

                log::debug!(
                    "CompType::FilenameExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    let mut builder = ActiveSuggestionsBuilder::new();
                    builder.extend_unprocessed(completions);
                    return Some(builder);
                }
            }
            tab_completion_context::CompType::FuzzyFilenameExpansion => {
                log::debug!(
                    "CompType::FuzzyFilenameExpansion for: {}",
                    word_under_cursor.as_ref()
                );
                let completions =
                    tab_complete_fuzzy_filename(word_under_cursor.as_ref(), comp_res_flags);

                log::debug!(
                    "CompType::FuzzyFilenameExpansion found {} completions for pattern: {}",
                    completions.len(),
                    word_under_cursor.as_ref()
                );
                if !completions.is_empty() {
                    let mut builder =
                        ActiveSuggestionsBuilder::new().with_auto_accept_if_solo(false);
                    builder.extend_unprocessed(completions);
                    return Some(builder);
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

fn tab_complete_first_word(command: &str) -> ActiveSuggestionsBuilder {
    log::debug!("Generating first word completions for: '{}'", command);
    let mut builder = ActiveSuggestionsBuilder::new();
    if command.is_empty() {
        return builder;
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        // Path to executable
        let files = tab_complete_glob_expansion(
            &(command.to_string() + "*"),
            bash_funcs::CompletionFlags::default(),
        );
        let executable_files = filter_out_non_executables(files);
        builder.extend_unprocessed(executable_files);
        return builder;
    }

    let mut res = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for poss_completion in bash_funcs::get_possible_command_words() {
        if poss_completion.starts_with(command) && seen.insert(poss_completion.clone()) {
            res.push(poss_completion);
        }
    }

    if res.is_empty() {
        return builder;
    }

    res.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    res.dedup();
    builder.extend_processed(ProcessedSuggestion::from_string_vec(res, "", " "));
    builder
}

fn tab_complete_fuzzy_first_word(command: &str) -> ActiveSuggestionsBuilder {
    log::debug!("Generating fuzzy first word completions for: '{}'", command);
    let mut builder = ActiveSuggestionsBuilder::new();
    if command.is_empty() {
        return builder;
    }

    if command.starts_with('.') || command.contains('/') || command.starts_with('~') {
        let fuzzy_files =
            tab_complete_fuzzy_filename(command, bash_funcs::CompletionFlags::default());
        let executable_files = filter_out_non_executables(fuzzy_files);
        builder.extend_unprocessed(executable_files);
        return builder;
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
            content_utils::fuzzy_match_with_threshold(
                &matcher,
                filename,
                &dequoted_fragment,
                content_utils::FuzzyMatchThreshold::Medium,
            )
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

/// Outcome of applying tab-completion results directly to a [`TextBuffer`].
///
/// This is the buffer-mutation half of `finish_tab_complete` factored out so
/// it can be exercised from unit tests without constructing a full `App`.
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
    if builder.len() == 1
        && builder.auto_accept_if_solo
        && let Some(suggestion) = builder.processed.iter().next()
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
    pub(crate) fn finish_tab_complete(
        &mut self,
        builder: ActiveSuggestionsBuilder,
        wuc_substring: SubString,
        load_time: std::time::Duration,
    ) {
        let outcome = apply_tab_complete_to_buffer(&mut self.buffer, &builder, &wuc_substring);
        match outcome {
            TabCompleteBufferOutcome::SoloAccepted => {
                self.content_mode = ContentMode::Normal;
            }
            TabCompleteBufferOutcome::Pending { final_wuc } => {
                let suggestions = ActiveSuggestions::new(builder, final_wuc, load_time);
                self.content_mode = ContentMode::TabCompletion(Box::new(suggestions));
            }
        }
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

        let start_time = std::time::Instant::now();

        let thread_handle = std::thread::spawn(move || {
            let result = gen_completions_internal(&completion_context_owned);
            if result.is_none() {
                log::debug!(
                    "No suggestions generated for completion context: {:?}",
                    completion_context_owned
                );
            }
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
    use crate::active_suggestions::ProcessedSuggestion;
    use crate::tab_completion_context::get_completion_context;
    use crate::text_buffer::TextBuffer;
    use rusty_fork::rusty_fork_test;

    const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

    /// Run completion against `command` (cursor placed at the end of the
    /// string), drain anything still queued, then return the processed
    /// suggestions sorted by `s` for stable comparison.
    fn run_completion(command: &str) -> Vec<ProcessedSuggestion> {
        let buffer = TextBuffer::new(command);
        let comp_context = get_completion_context(buffer.buffer(), buffer.cursor_byte_pos());
        let Some(builder) = gen_completions_internal(&comp_context) else {
            return Vec::new();
        };
        let mut suggestions: Vec<ProcessedSuggestion> = builder.processed;
        suggestions.sort_by(|a, b| a.s.cmp(&b.s));
        suggestions
    }

    fn assert_completions(command: &str, expected: &[ProcessedSuggestion]) {
        let actual = run_completion(command);
        assert_eq!(
            actual.len(),
            expected.len(),
            "completion count mismatch for command {:?}: got {:?}, expected {:?}",
            command,
            actual,
            expected
        );
        for (got, want) in actual.iter().zip(expected.iter()) {
            assert_eq!(
                (&got.prefix, &got.s, &got.suffix),
                (&want.prefix, &want.s, &want.suffix),
                "for command {:?}: got {:?}, expected {:?}",
                command,
                got,
                want
            );
        }
    }

    fn cd_to_example_fs() {
        let dir = format!("{}/tests/example_fs", MANIFEST_DIR);
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
        // No need to set the `PWD` env var: the `#[cfg(test)]` bash_funcs
        // (in particular `get_envvar_value` / `expand_filename`) source
        // `$PWD` from the process's current working directory via
        // `bash_funcs::test_fixtures::test_env_vars`.
    }

    fn cd_to_example_braces_fs() {
        let dir = format!("{}/tests/example_braces_fs", MANIFEST_DIR);
        std::env::set_current_dir(&dir).unwrap_or_else(|e| panic!("cd {dir}: {e}"));
    }

    rusty_fork_test! {
        // ------- dummy git completion (clap-based, no bash symbols) -------

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
            let builder = gen_completions_internal(&comp_context).expect("some completions");
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
                    ProcessedSuggestion::new("./abc/", "", ""),
                    ProcessedSuggestion::new("./bar.txt", "", " "),
                    ProcessedSuggestion::new(r"./file\ with\ spaces.txt", "", " "),
                    ProcessedSuggestion::new("./foo/", "", ""),
                    ProcessedSuggestion::new(r"./many\ spaces\ here/", "", ""),
                    ProcessedSuggestion::new("./sym_link_to_foo/", "", ""),
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

        // ------- finish_tab_complete (auto-accept solo) ------------------

        #[test]
        fn finish_tab_complete_auto_accepts_solo_suggestion() {
            cd_to_example_fs();
            let mut buffer = TextBuffer::new("mycmd bar.tx");
            let comp_context =
                get_completion_context(buffer.buffer(), buffer.cursor_byte_pos());
            let wuc = comp_context.word_under_cursor.clone();
            let builder = gen_completions_internal(&comp_context).expect("some completions");

            assert_eq!(builder.len(), 1, "expected exactly one suggestion");
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
            let comp_context =
                get_completion_context(buffer.buffer(), buffer.cursor_byte_pos());
            let wuc = comp_context.word_under_cursor.clone();
            let builder = gen_completions_internal(&comp_context).expect("some completions");
            assert!(builder.len() >= 2, "expected multiple suggestions, got {}", builder.len());
            let outcome = apply_tab_complete_to_buffer(&mut buffer, &builder, &wuc);
            assert!(matches!(outcome, TabCompleteBufferOutcome::Pending { .. }));
            assert_eq!(buffer.buffer(), "mycmd foo");
        }
    }
}
