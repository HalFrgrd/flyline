use std::collections::HashMap;
use std::path::Path;

use crate::grammar::{QuoteType, find_quote_type};

/* Values for COMPSPEC options field. */
// In bash >= 4.4, COPT_NOQUOTE was inserted at (1<<4), shifting later values.
// In bash < 4.4: NOSPACE=(1<<4), BASHDEFAULT=(1<<5), PLUSDIRS=(1<<6)
// In bash >= 4.4: NOQUOTE=(1<<4), NOSPACE=(1<<5), BASHDEFAULT=(1<<6), PLUSDIRS=(1<<7), NOSORT=(1<<8), FULLQUOTE=(1<<9)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompspecOption {
    Reserved = 1 << 0,
    Default = 1 << 1,
    Filenames = 1 << 2,
    Dirnames = 1 << 3,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoQuote = 1 << 4,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoSpace = 1 << 5,
    #[cfg(not(feature = "pre_bash_4_4"))]
    BashDefault = 1 << 6,
    #[cfg(not(feature = "pre_bash_4_4"))]
    PlusDirs = 1 << 7,
    #[cfg(not(feature = "pre_bash_4_4"))]
    NoSort = 1 << 8,
    #[cfg(not(feature = "pre_bash_4_4"))]
    FullQuote = 1 << 9,
    #[cfg(feature = "pre_bash_4_4")]
    NoSpace = 1 << 4,
    #[cfg(feature = "pre_bash_4_4")]
    BashDefault = 1 << 5,
    #[cfg(feature = "pre_bash_4_4")]
    PlusDirs = 1 << 6,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletionFlags {
    pub quote_type: Option<QuoteType>,

    pub readline_default_fallback_desired: bool,
    // pub dirnames_desired: bool, // Bash handles this already during call to programmable_completions
    // pub plus_dirs: bool, // Likewise
    pub filename_quoting_desired: bool,
    pub filename_completion_desired: bool,
    pub no_suffix_desired: bool,
    pub suffix_character: char,
    pub bash_default_fallback_desired: bool,
    pub nosort_desired: bool,
    // pub full_quote: bool,
    pub some_dont_end_in_equal_sign: bool,
}

impl CompletionFlags {
    pub fn from(
        quote_type: Option<QuoteType>,
        foundcs: libc::c_int,
        append_char: i32,
        some_dont_end_in_equal_sign: bool,
    ) -> Self {
        Self {
            quote_type,
            readline_default_fallback_desired: foundcs & (CompspecOption::Default as libc::c_int)
                != 0,
            #[cfg(not(feature = "pre_bash_4_4"))]
            filename_quoting_desired: foundcs & (CompspecOption::NoQuote as libc::c_int) == 0,
            #[cfg(feature = "pre_bash_4_4")]
            filename_quoting_desired: true,
            filename_completion_desired: foundcs & (CompspecOption::Filenames as libc::c_int) != 0,
            no_suffix_desired: foundcs & (CompspecOption::NoSpace as libc::c_int) != 0,
            suffix_character: char::from_u32(append_char as u32).unwrap_or(' '),
            bash_default_fallback_desired: foundcs & (CompspecOption::BashDefault as libc::c_int)
                != 0,
            #[cfg(not(feature = "pre_bash_4_4"))]
            nosort_desired: foundcs & (CompspecOption::NoSort as libc::c_int) != 0,
            #[cfg(feature = "pre_bash_4_4")]
            nosort_desired: false,
            some_dont_end_in_equal_sign,
        }
    }

    pub fn from_alt(word_under_cursor: &str, completions: &[String]) -> Self {
        let mut flags = Self::default();
        flags.quote_type = find_quote_type(word_under_cursor);
        flags.some_dont_end_in_equal_sign = completions.iter().any(|s| !s.ends_with('='));
        flags
    }
}

impl Default for CompletionFlags {
    fn default() -> Self {
        Self {
            quote_type: None,
            readline_default_fallback_desired: true,
            filename_quoting_desired: true,
            filename_completion_desired: false,
            no_suffix_desired: false,
            suffix_character: ' ',
            bash_default_fallback_desired: false,
            nosort_desired: false,
            some_dont_end_in_equal_sign: false,
        }
    }
}

pub struct ProgrammableCompleteReturn {
    pub completions: Vec<String>,
    pub flags: CompletionFlags,
    pub compspec_was_useful: bool,
}

impl std::fmt::Debug for ProgrammableCompleteReturn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MAX_DISPLAY: usize = 50;
        let mut s = f.debug_struct("ProgrammableCompleteReturn");
        if self.completions.len() <= MAX_DISPLAY {
            s.field("completions", &self.completions);
        } else {
            s.field(
                "completions",
                &format_args!(
                    "({} total, showing first {}) {:?}",
                    self.completions.len(),
                    MAX_DISPLAY,
                    &self.completions[..MAX_DISPLAY]
                ),
            );
        }
        s.field("flags", &self.flags)
            .field("compspec_was_useful", &self.compspec_was_useful)
            .finish()
    }
}

fn is_strict_completion_value(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let first = val.chars().next().unwrap();
    if !first.is_ascii_alphanumeric() && first != '-' {
        return false;
    }
    val.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == ':'
            || c == '.'
            || c == '/'
            || c == '@'
    })
}

fn analyze_candidate(s: &str) -> Option<(&str, &str, usize)> {
    if s.contains('\t') {
        return None;
    }
    if let Some(pos) = s.find("  ") {
        let value = &s[..pos];
        let rest = &s[pos..];
        let description = rest.trim_start();
        let desc_start_index = s.len() - rest.len() + (rest.len() - description.len());

        if is_strict_completion_value(value) && !description.is_empty() {
            return Some((value, description, desc_start_index));
        }
    }
    None
}

fn should_infer_filename_completion(completions: &[String], flags: &CompletionFlags) -> bool {
    if flags.filename_completion_desired
        || completions.is_empty()
        || completions.len() >= crate::FILENAME_INFERENCE_LIMIT
    {
        return false;
    }

    completions.iter().all(|completion| {
        !completion.contains('\t')
            && Path::new(&crate::shell::backend().expand_path(completion)).exists()
    })
}

/// Some completion scripts like gh or docker put descriptions inline with
/// the suggestion when there are multiple suggestions.
/// So here I convert those to the format "suggestion<TAB>description" so that
/// flyline can show the description in a separate column.
pub fn detect_and_convert_inline_descriptions(
    completions: &mut Vec<String>,
    flags: &CompletionFlags,
) {
    if flags.filename_completion_desired || completions.iter().any(|s| s.contains('\t')) {
        return;
    }

    let mut detected = false;

    if completions.len() == 1 {
        if let Some((value, description, _)) = analyze_candidate(&completions[0])
            && (description.contains(' ') || value.starts_with('-')) {
                detected = true;
            }
    } else if completions.len() > 1 {
        let mut desc_columns = HashMap::new();

        for s in completions.iter() {
            if let Some((_, _, col)) = analyze_candidate(s) {
                *desc_columns.entry(col).or_insert(0) += 1;
            }
        }

        let has_aligned = desc_columns.values().any(|&count| count >= 2);

        if has_aligned {
            detected = true;
        }
    }

    if detected {
        for s in completions.iter_mut() {
            if let Some((value, description, _)) = analyze_candidate(s) {
                let description = if let Some(stripped) = description
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                {
                    stripped
                } else {
                    description
                };
                *s = format!("{}\t{}", value, description);
            }
        }
    }
}

impl ProgrammableCompleteReturn {
    pub fn new(
        mut completions: Vec<String>,
        mut flags: CompletionFlags,
        compspec_was_useful: bool,
    ) -> Self {
        if should_infer_filename_completion(&completions, &flags) {
            flags.filename_completion_desired = true;
        }
        detect_and_convert_inline_descriptions(&mut completions, &flags);
        Self {
            completions,
            flags,
            compspec_was_useful,
        }
    }

    pub fn from(
        completions: Vec<String>,
        quote_type: Option<QuoteType>,
        foundcs: libc::c_int,
        append_char: i32,
        compspec_was_useful: bool,
    ) -> Self {
        let some_dont_end_in_equal_sign = completions.iter().any(|s| !s.ends_with('='));
        Self::new(
            completions,
            CompletionFlags::from(
                quote_type,
                foundcs,
                append_char,
                some_dont_end_in_equal_sign,
            ),
            compspec_was_useful,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_and_convert_inline_descriptions() {
        let mut flags = CompletionFlags::default();
        flags.filename_completion_desired = false;

        // 1. A typical aligned list of options with descriptions.
        let mut comps = vec![
            "port      List port mappings".to_string(),
            "ps        List containers".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps\tList containers");

        // 2. An aligned list of options where some descriptions are single words.
        let mut comps = vec![
            "-d      Decompress".to_string(),
            "-z      Compress".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "-d\tDecompress");
        assert_eq!(comps[1], "-z\tCompress");

        // 3. A single option with a description (containing a space).
        let mut comps = vec!["port      List port mappings".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");

        // 4. A single option with a single-word description starting with a flag.
        let mut comps = vec!["-d      Decompress".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "-d\tDecompress");

        // 5. A single option with a single-word description (not a flag).
        // Should NOT convert to avoid false positives (e.g. "my  file.txt" or "build  Build" when it is the only completion).
        let mut comps = vec!["build      Build".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "build      Build");

        // 6. Filename completion desired - should skip.
        let mut comps = vec![
            "port      List port mappings".to_string(),
            "ps        List containers".to_string(),
        ];
        let mut file_flags = CompletionFlags::default();
        file_flags.filename_completion_desired = true;
        detect_and_convert_inline_descriptions(&mut comps, &file_flags);
        assert_eq!(comps[0], "port      List port mappings");

        // 7. Non-aligned filenames (different lengths, double spaces).
        // Should NOT convert.
        let mut comps = vec!["my  file.txt".to_string(), "another  file.txt".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "my  file.txt");
        assert_eq!(comps[1], "another  file.txt");

        // 8. Arbitrary string with spaces in the value part (e.g. "my file      description").
        // Value part contains spaces, so it's not a strict completion value, should NOT convert.
        let mut comps = vec!["my file      description".to_string()];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "my file      description");

        // 9. One completion already contains a tab character.
        // Should NOT convert any of them.
        let mut comps = vec![
            "port\tList port mappings".to_string(),
            "ps      List containers".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps      List containers");

        // 10. Descriptions wrapped in parentheses should be stripped.
        let mut comps = vec![
            "port      (List port mappings)".to_string(),
            "ps        (List containers)".to_string(),
        ];
        detect_and_convert_inline_descriptions(&mut comps, &flags);
        assert_eq!(comps[0], "port\tList port mappings");
        assert_eq!(comps[1], "ps\tList containers");
    }
}
