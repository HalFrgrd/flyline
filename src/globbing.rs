use std::path::{Path, PathBuf};

use crate::grammar::{
    DParser, QuoteType, TokenKind, dequoting_function_rust, quoting_function_rust,
};
use crate::shell;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GlobPatternSplit<'a> {
    pub raw_prefix: &'a str,
    pub rhs_pattern: &'a str,
    pub has_glob: bool,
}

pub fn is_glob_pattern(s: &str) -> bool {
    split_glob_pattern(s).has_glob
}

pub fn split_glob_pattern(s: &str) -> GlobPatternSplit<'_> {
    let first_glob_pos = first_glob_pos(s);
    let search_end = first_glob_pos.unwrap_or(s.len());

    let (raw_prefix, rhs_pattern) = match s[..search_end].rfind('/') {
        Some(0) => (&s[..1], &s[1..]),
        Some(slash_pos) => (&s[..slash_pos], &s[slash_pos + 1..]),
        None => ("", s),
    };

    GlobPatternSplit {
        raw_prefix,
        rhs_pattern,
        has_glob: first_glob_pos.is_some(),
    }
}

fn first_glob_pos(s: &str) -> Option<usize> {
    let tokens = DParser::parse_and_annotate(s);
    for annotated in &tokens {
        if annotated.annotations.is_glob {
            match &annotated.token.kind {
                TokenKind::Word(val) => {
                    if let Some(offset) = DParser::first_unescaped_wildcard(val) {
                        return Some(annotated.token.byte_range().start + offset);
                    }
                }
                _ => {
                    return Some(annotated.token.byte_range().start);
                }
            }
        }
    }

    None
}

#[derive(Debug)]
pub(crate) struct PathPatternExpansion {
    /// The part of the pattern before the last '/' that separates the pattern kept in its original form
    /// (e.g. `~/foo` for `~/foo/baz*` or `relative/dir` for `relative/dir/*/*.txt`).
    /// it might be empty : e.g. `baz*`
    raw_prefix: String,
    /// `raw_prefix` after tilde expansion, conversion to an absolute path, and
    /// environment-variable expansion (e.g. `/home/user/foo` or `/cwd/relative/dir`).
    /// it might be empty: e.g. `/pro*/123*`.
    expanded_prefix: String,
    /// The part of the pattern after the separating`/`— the glob portion
    /// (e.g. `baz*` or `*/*.txt`).
    rhs_pattern: String,
}

impl PathPatternExpansion {
    pub(crate) fn new(pattern: &str) -> Self {
        let split = split_glob_pattern(pattern);
        let raw_prefix = split.raw_prefix.to_string();
        let rhs_pattern = split.rhs_pattern.to_string();
        let expanded_prefix = shell::backend().expand_path(&raw_prefix);

        let rhs_pattern = dequoting_function_rust(&rhs_pattern);

        PathPatternExpansion {
            raw_prefix,
            expanded_prefix,
            rhs_pattern,
        }
    }

    /// Build the glob pattern(s) used to match against the filesystem.
    ///
    /// The returned vector contains the cartesian product of any brace
    /// expansions present in the pattern (e.g. `foo*{1,3}/bar*{A,C}`
    /// expands to four patterns). When the pattern contains no brace
    /// alternatives, the returned vector has a single element.
    pub(crate) fn glob_pattern(&self) -> Vec<String> {
        let combined = join_path_parts(&self.expanded_prefix, &self.rhs_pattern);
        expand_braces(&combined)
    }

    /// Perform segment-by-segment filesystem expansion across all brace-expanded patterns.
    pub(crate) fn expand(&self) -> Vec<PathBuf> {
        let mut results = Vec::new();
        let patterns = self.glob_pattern();
        let wants_hidden = self.wants_hidden();
        for pat in patterns {
            let matches = glob_expand(&pat, wants_hidden);
            results.extend(matches);
        }
        results.sort();
        results.dedup();
        results
    }

    pub(crate) fn wants_hidden(&self) -> bool {
        self.rhs_pattern.starts_with('.') && !self.rhs_pattern.starts_with("./")
    }

    pub(crate) fn convert_expanded_match_to_unexpanded(
        &self,
        expanded_match: &str,
        quote_type: Option<QuoteType>,
    ) -> (String, String) {
        if self.expanded_prefix.is_empty() {
            let quoted_rhs =
                quoting_function_rust(expanded_match, quote_type.unwrap_or_default(), false, false);
            let combined = join_path_parts(&self.raw_prefix, &quoted_rhs);
            return (combined, quoted_rhs);
        }

        let expected_prefix = if self.expanded_prefix.ends_with('/') {
            self.expanded_prefix.clone()
        } else {
            format!("{}/", self.expanded_prefix)
        };

        if let Some(rhs) = expanded_match.strip_prefix(&expected_prefix) {
            let quoted_rhs =
                quoting_function_rust(rhs, quote_type.unwrap_or_default(), false, false);
            let combined = join_path_parts(&self.raw_prefix, &quoted_rhs);
            (combined.clone(), quoted_rhs)
        } else {
            log::warn!(
                "Expected expanded match '{}' to start with expanded_prefix '{}', but it did not.",
                expanded_match,
                expected_prefix
            );
            (expanded_match.to_string(), expanded_match.to_string())
        }
    }
}

fn join_path_parts(prefix: &str, rhs: &str) -> String {
    if rhs.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        rhs.to_string()
    } else if prefix.ends_with('/') {
        format!("{prefix}{rhs}")
    } else {
        format!("{prefix}/{rhs}")
    }
}

/// Matches `text` against a Bash glob / extglob `pattern`.
///
/// Supports:
/// - `*` (matches zero or more characters)
/// - `?` (matches any single character)
/// - `[...]`, `[!...]`, `[^...]` (character sets, ranges, and negation)
/// - `[[:class:]]` (POSIX character classes)
/// - `?(p1|p2)` (matches zero or one occurrence of the patterns)
/// - `*(p1|p2)` (matches zero or more occurrences of the patterns)
/// - `+(p1|p2)` (matches one or more occurrences of the patterns)
/// - `@(p1|p2)` (matches exactly one of the patterns)
/// - `!(p1|p2)` (matches anything except one of the patterns)
/// - `\c` (escaped character matches `c` literally)
pub fn extglob_match(pattern: &str, text: &str) -> bool {
    extglob_match_inner(pattern, text)
}

fn extglob_match_inner(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    let first_char = pattern.chars().next().unwrap();

    // 1. Escaped character
    if first_char == '\\' {
        let rest = &pattern[1..];
        if rest.is_empty() {
            return text == "\\";
        }
        let escaped_char = rest.chars().next().unwrap();
        if let Some(text_char) = text.chars().next()
            && text_char == escaped_char
        {
            return extglob_match_inner(
                &rest[escaped_char.len_utf8()..],
                &text[text_char.len_utf8()..],
            );
        }
        return false;
    }

    // 2. Extglob operator: ?(pat), *(pat), +(pat), @(pat), !(pat)
    if matches!(first_char, '?' | '*' | '+' | '@' | '!')
        && pattern[first_char.len_utf8()..].starts_with('(')
        && let Some((end_idx, alternatives)) = parse_extglob(pattern)
    {
        let prest = &pattern[end_idx + 1..];
        match first_char {
            '?' => {
                // Match 0 occurrences:
                if extglob_match_inner(prest, text) {
                    return true;
                }
                // Match 1 occurrence:
                for split_idx in text_split_points(text) {
                    let prefix = &text[..split_idx];
                    let suffix = &text[split_idx..];
                    for alt in &alternatives {
                        if extglob_match_inner(alt, prefix) && extglob_match_inner(prest, suffix) {
                            return true;
                        }
                    }
                }
                return false;
            }
            '@' => {
                // Match exactly 1 occurrence:
                for split_idx in text_split_points(text) {
                    let prefix = &text[..split_idx];
                    let suffix = &text[split_idx..];
                    for alt in &alternatives {
                        if extglob_match_inner(alt, prefix) && extglob_match_inner(prest, suffix) {
                            return true;
                        }
                    }
                }
                return false;
            }
            '+' => {
                // Match 1 or more occurrences:
                return match_plus_extglob(&alternatives, pattern, prest, text);
            }
            '*' => {
                // Match 0 occurrences:
                if extglob_match_inner(prest, text) {
                    return true;
                }
                // Match 1 or more occurrences:
                return match_plus_extglob(&alternatives, pattern, prest, text);
            }
            '!' => {
                // Match anything EXCEPT one of the patterns:
                for split_idx in text_split_points(text) {
                    let prefix = &text[..split_idx];
                    let suffix = &text[split_idx..];
                    let mut matched_any = false;
                    for alt in &alternatives {
                        if extglob_match_inner(alt, prefix) {
                            matched_any = true;
                            break;
                        }
                    }
                    if !matched_any && extglob_match_inner(prest, suffix) {
                        return true;
                    }
                }
                return false;
            }
            _ => unreachable!(),
        }
    }

    // 3. Asterisk wildcard: *
    if first_char == '*' {
        let mut rest_idx = 1;
        while rest_idx < pattern.len() && pattern.as_bytes()[rest_idx] == b'*' {
            rest_idx += 1;
        }
        let rest_pattern = &pattern[rest_idx..];
        if rest_pattern.is_empty() {
            return true;
        }
        for split_idx in text_split_points(text) {
            let suffix = &text[split_idx..];
            if extglob_match_inner(rest_pattern, suffix) {
                return true;
            }
        }
        return false;
    }

    // 4. Question mark wildcard: ?
    if first_char == '?' {
        if text.is_empty() {
            return false;
        }
        let text_char = text.chars().next().unwrap();
        return extglob_match_inner(&pattern[1..], &text[text_char.len_utf8()..]);
    }

    // 5. Bracket expression: [...]
    if first_char == '['
        && let Some((end_idx, is_negated, spec)) = parse_bracket_class(pattern)
    {
        if text.is_empty() {
            return false;
        }
        let text_char = text.chars().next().unwrap();
        let matched = match_bracket_spec(spec, text_char);
        let is_match = if is_negated { !matched } else { matched };
        if is_match {
            return extglob_match_inner(&pattern[end_idx + 1..], &text[text_char.len_utf8()..]);
        }
        return false;
    }

    // 6. Literal character
    if let Some(text_char) = text.chars().next()
        && text_char == first_char
    {
        return extglob_match_inner(
            &pattern[first_char.len_utf8()..],
            &text[text_char.len_utf8()..],
        );
    }

    false
}

fn text_split_points(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(text.len()))
}

fn match_plus_extglob(
    alternatives: &[String],
    full_pattern: &str,
    prest: &str,
    text: &str,
) -> bool {
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let split_points: Vec<usize> = if text.is_empty() {
        vec![0]
    } else {
        (1..=char_indices.len())
            .map(|k| {
                if k == char_indices.len() {
                    text.len()
                } else {
                    char_indices[k].0
                }
            })
            .collect()
    };

    for split_idx in split_points {
        let prefix = &text[..split_idx];
        let suffix = &text[split_idx..];

        for alt in alternatives {
            if extglob_match_inner(alt, prefix) {
                if extglob_match_inner(prest, suffix) {
                    return true;
                }
                if split_idx > 0 && extglob_match_inner(full_pattern, suffix) {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_extglob(pattern: &str) -> Option<(usize, Vec<String>)> {
    let bytes = pattern.as_bytes();
    if bytes.len() < 3 || bytes[1] != b'(' {
        return None;
    }
    let mut depth = 0;
    let mut alt_start = 2;
    let mut alternatives = Vec::new();
    let mut i = 1;

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'['
            && let Some((end_bracket, _, _)) = parse_bracket_class(&pattern[i..])
        {
            i += end_bracket + 1;
            continue;
        }
        if bytes[i] == b'(' {
            depth += 1;
        } else if bytes[i] == b')' {
            depth -= 1;
            if depth == 0 {
                alternatives.push(pattern[alt_start..i].to_string());
                return Some((i, alternatives));
            }
        } else if bytes[i] == b'|' && depth == 1 {
            alternatives.push(pattern[alt_start..i].to_string());
            alt_start = i + 1;
        }
        i += 1;
    }
    None
}

fn parse_bracket_class(pattern: &str) -> Option<(usize, bool, &str)> {
    let bytes = pattern.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'[' {
        return None;
    }

    let mut i = 1;
    let mut is_negated = false;
    if i < bytes.len() && (bytes[i] == b'!' || bytes[i] == b'^') {
        is_negated = true;
        i += 1;
    }

    let content_start = i;
    // POSIX rule: if ']' is the first char in the class, it's literal
    if i < bytes.len() && bytes[i] == b']' {
        i += 1;
    }

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i..].starts_with(b"[:")
            && let Some(pos) = pattern[i + 2..].find(":]")
        {
            i += 2 + pos + 2;
            continue;
        }
        if bytes[i] == b']' {
            return Some((i, is_negated, &pattern[content_start..i]));
        }
        i += 1;
    }

    None
}

fn match_bracket_spec(spec: &str, c: char) -> bool {
    let bytes = spec.as_bytes();
    let mut i = 0;

    // Handle leading ']' if present
    if !spec.is_empty() && bytes[0] == b']' {
        if c == ']' {
            return true;
        }
        i = 1;
    }

    let chars: Vec<char> = spec[i..].chars().collect();
    let mut ci = 0;
    while ci < chars.len() {
        let remaining: String = chars[ci..].iter().collect();
        if let Some(stripped) = remaining.strip_prefix("[:")
            && let Some(end_pos) = stripped.find(":]")
        {
            let class_name = &stripped[..end_pos];
            if posix_class_match(class_name, c) {
                return true;
            }
            let skip_count = 2 + end_pos + 2;
            let char_skip = remaining[..skip_count].chars().count();
            ci += char_skip;
            continue;
        }

        let current_char = if chars[ci] == '\\' && ci + 1 < chars.len() {
            ci += 1;
            chars[ci]
        } else {
            chars[ci]
        };

        if ci + 2 < chars.len() && chars[ci + 1] == '-' && chars[ci + 2] != ']' {
            let end_char = if chars[ci + 2] == '\\' && ci + 3 < chars.len() {
                ci += 1;
                chars[ci + 2]
            } else {
                chars[ci + 2]
            };
            if c >= current_char && c <= end_char {
                return true;
            }
            ci += 3;
        } else {
            if c == current_char {
                return true;
            }
            ci += 1;
        }
    }

    false
}

fn posix_class_match(class_name: &str, c: char) -> bool {
    match class_name {
        "alnum" => c.is_alphanumeric(),
        "alpha" => c.is_alphabetic(),
        "ascii" => c.is_ascii(),
        "blank" => c == ' ' || c == '\t',
        "cntrl" => c.is_control(),
        "digit" => c.is_ascii_digit(),
        "graph" => c.is_ascii_graphic(),
        "lower" => c.is_lowercase(),
        "print" => c.is_ascii_graphic() || c == ' ',
        "punct" => c.is_ascii_punctuation(),
        "space" => c.is_whitespace(),
        "upper" => c.is_uppercase(),
        "word" => c.is_alphanumeric() || c == '_',
        "xdigit" => c.is_ascii_hexdigit(),
        _ => false,
    }
}

pub fn has_glob_or_extglob(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'*' || bytes[i] == b'?' || bytes[i] == b'[' {
            return true;
        }
        if i + 1 < bytes.len()
            && matches!(bytes[i], b'?' | b'*' | b'+' | b'@' | b'!')
            && bytes[i + 1] == b'('
        {
            return true;
        }
        i += 1;
    }
    false
}

pub fn split_path_segments(pattern: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let bytes = pattern.as_bytes();
    let mut start = 0;
    let mut i = 0;
    let mut paren_depth = 0;
    let mut in_bracket = false;

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            in_bracket = true;
        } else if bytes[i] == b']' {
            in_bracket = false;
        } else if (i + 1 < bytes.len()
            && matches!(bytes[i], b'?' | b'*' | b'+' | b'@' | b'!')
            && bytes[i + 1] == b'(')
            || (bytes[i] == b'(' && paren_depth > 0)
        {
            if bytes[i] != b'(' {
                i += 1;
            }
            paren_depth += 1;
        } else if bytes[i] == b')' && paren_depth > 0 {
            paren_depth -= 1;
        } else if bytes[i] == b'/' && paren_depth == 0 && !in_bracket {
            if i > start {
                segments.push(&pattern[start..i]);
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < pattern.len() {
        segments.push(&pattern[start..]);
    }
    segments
}

/// Expand a full glob pattern against the filesystem, returning all matching paths.
pub fn glob_expand(pattern: &str, wants_hidden: bool) -> Vec<PathBuf> {
    if pattern.is_empty() {
        return Vec::new();
    }

    let is_absolute = pattern.starts_with('/');
    let clean_pattern = if is_absolute {
        pattern.trim_start_matches('/')
    } else {
        pattern
    };

    let segments = split_path_segments(clean_pattern);
    if segments.is_empty() {
        if is_absolute {
            return vec![PathBuf::from("/")];
        }
        return Vec::new();
    }

    let initial_paths: Vec<PathBuf> = if is_absolute {
        vec![PathBuf::from("/")]
    } else {
        vec![PathBuf::from("")]
    };

    let mut current_paths = initial_paths;

    for (seg_idx, segment) in segments.iter().enumerate() {
        let is_last = seg_idx == segments.len() - 1;
        let mut next_paths = Vec::new();

        if segment.is_empty() {
            continue;
        }

        let is_glob_seg = has_glob_or_extglob(segment);

        for base_dir in current_paths {
            let dir_to_read = if base_dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                base_dir.as_path()
            };

            if !is_glob_seg {
                let candidate = if base_dir.as_os_str().is_empty() {
                    PathBuf::from(segment)
                } else if base_dir == Path::new("/") {
                    PathBuf::from(format!("/{}", segment))
                } else {
                    base_dir.join(segment)
                };

                if is_last {
                    if candidate.exists() || candidate.is_symlink() {
                        next_paths.push(candidate);
                    }
                } else if candidate.is_dir() {
                    next_paths.push(candidate);
                }
            } else {
                let Ok(entries) = std::fs::read_dir(dir_to_read) else {
                    continue;
                };

                let seg_wants_hidden = wants_hidden || segment.starts_with('.');

                for entry in entries.filter_map(Result::ok) {
                    let file_name = entry.file_name();
                    let file_name_str = file_name.to_string_lossy();

                    if file_name_str == "." || file_name_str == ".." {
                        continue;
                    }

                    if !seg_wants_hidden && file_name_str.starts_with('.') {
                        continue;
                    }

                    if extglob_match(segment, &file_name_str) {
                        let full_path = if base_dir.as_os_str().is_empty() {
                            PathBuf::from(file_name_str.as_ref())
                        } else if base_dir == Path::new("/") {
                            PathBuf::from(format!("/{}", file_name_str))
                        } else {
                            base_dir.join(file_name_str.as_ref())
                        };

                        if is_last || entry.path().is_dir() {
                            next_paths.push(full_path);
                        }
                    }
                }
            }
        }

        current_paths = next_paths;
        if current_paths.is_empty() {
            break;
        }
    }

    current_paths.sort();
    current_paths.dedup();
    current_paths
}

/// Expand bash-style brace alternatives in `pattern` (the `{a,b,c}` form).
///
/// Returns the cartesian product of all top-level brace groups. Brace groups
/// may be nested, in which case the inner alternatives are expanded first.
/// A brace group must contain at least one unescaped top-level comma to be
/// treated as an alternation; otherwise the braces are left untouched (this
/// matches bash's behaviour for things like `${VAR}` or `{single}`).
///
/// Sequence expressions like `{1..5}` are intentionally NOT supported here —
/// only comma-separated alternatives, which is what tab completion needs to
/// drive glob expansion from a pattern such as `foo*{1,3}/bar*{A,C}`.
///
/// When `pattern` contains no expandable braces, the returned vector contains
/// `pattern` unchanged.
fn expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'{'
            && let Some((end, alternatives)) = find_brace_alternatives(pattern, i)
        {
            let prefix = &pattern[..i];
            let suffix = &pattern[end + 1..];
            let suffix_expansions = expand_braces(suffix);
            let mut out = Vec::new();
            for alt in &alternatives {
                for alt_expanded in expand_braces(alt) {
                    for suf in &suffix_expansions {
                        out.push(format!("{}{}{}", prefix, alt_expanded, suf));
                    }
                }
            }
            return out;
        }
        i += 1;
    }
    vec![pattern.to_string()]
}

/// Given that `pattern[start]` is an unescaped `{`, look for the matching `}`
/// at the same nesting level. If found, and there is at least one top-level
/// (unescaped, un-nested) comma between them, return the index of the closing
/// `}` together with the list of alternatives. Otherwise return `None`.
fn find_brace_alternatives(pattern: &str, start: usize) -> Option<(usize, Vec<String>)> {
    let bytes = pattern.as_bytes();
    debug_assert_eq!(bytes[start], b'{');
    let mut depth: i32 = 0;
    let mut alt_start = start + 1;
    let mut alternatives: Vec<String> = Vec::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    alternatives.push(pattern[alt_start..i].to_string());
                    if alternatives.len() < 2 {
                        // No top-level comma -> not a brace alternation.
                        return None;
                    }
                    return Some((i, alternatives));
                }
            }
            b',' if depth == 1 => {
                alternatives.push(pattern[alt_start..i].to_string());
                alt_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    // Unmatched '{'.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `PathPatternExpansion` directly from its fields without going
    /// through `PathPatternExpansion::new`, which would require bash symbols
    /// at link time. Used to unit-test `glob_pattern` in isolation.
    fn make_expansion(expanded_prefix: &str, rhs_pattern: &str) -> PathPatternExpansion {
        PathPatternExpansion {
            raw_prefix: String::new(),
            expanded_prefix: expanded_prefix.to_string(),
            rhs_pattern: rhs_pattern.to_string(),
        }
    }

    #[test]
    fn split_glob_pattern_with_glob_segments() {
        assert_eq!(
            split_glob_pattern("./foo*"),
            GlobPatternSplit {
                raw_prefix: ".",
                rhs_pattern: "foo*",
                has_glob: true,
            },
        );
        assert_eq!(
            split_glob_pattern("./{foo,bar}.txt"),
            GlobPatternSplit {
                raw_prefix: ".",
                rhs_pattern: "{foo,bar}.txt",
                has_glob: true,
            },
        );
        assert_eq!(
            split_glob_pattern("src/{foo,bar}/baz*.rs"),
            GlobPatternSplit {
                raw_prefix: "src",
                rhs_pattern: "{foo,bar}/baz*.rs",
                has_glob: true,
            },
        );
        assert_eq!(
            split_glob_pattern("/tmp/foo*/bar"),
            GlobPatternSplit {
                raw_prefix: "/tmp",
                rhs_pattern: "foo*/bar",
                has_glob: true,
            },
        );
        assert_eq!(
            split_glob_pattern("/foo*"),
            GlobPatternSplit {
                raw_prefix: "/",
                rhs_pattern: "foo*",
                has_glob: true,
            },
        );
        assert_eq!(
            split_glob_pattern("src/@(app|grammar)/*.rs"),
            GlobPatternSplit {
                raw_prefix: "src",
                rhs_pattern: "@(app|grammar)/*.rs",
                has_glob: true,
            },
        );
    }

    #[test]
    fn split_glob_pattern_without_glob_segments() {
        assert_eq!(
            split_glob_pattern("src/lib.rs"),
            GlobPatternSplit {
                raw_prefix: "src",
                rhs_pattern: "lib.rs",
                has_glob: false,
            },
        );
        assert_eq!(
            split_glob_pattern("plain"),
            GlobPatternSplit {
                raw_prefix: "",
                rhs_pattern: "plain",
                has_glob: false,
            },
        );
    }

    #[test]
    fn is_glob_pattern_detects_supported_patterns() {
        assert!(is_glob_pattern("./foo*"));
        assert!(is_glob_pattern("./foo?.txt"));
        assert!(is_glob_pattern("./foo[ab].txt"));
        assert!(is_glob_pattern("./{foo,bar}.txt"));
        assert!(is_glob_pattern("./foo{1..3}.txt"));
        assert!(is_glob_pattern("./{foo,bar}/{baz,qux}.txt"));
        assert!(is_glob_pattern("./foo@(bar|baz).txt"));
        assert!(is_glob_pattern("./foo?(bar).txt"));
        assert!(is_glob_pattern("./foo*(bar).txt"));
        assert!(is_glob_pattern("./foo+(bar).txt"));
        assert!(is_glob_pattern("./foo!(bar).txt"));
        assert!(is_glob_pattern("@(a|b)"));
        assert!(is_glob_pattern("!(target)/*.rs"));
    }

    #[test]
    fn is_glob_pattern_ignores_literal_or_incomplete_patterns() {
        assert!(!is_glob_pattern(r"./foo\*"));
        assert!(!is_glob_pattern(r"./foo\?.txt"));
        assert!(!is_glob_pattern(r"./foo\[ab].txt"));
        assert!(!is_glob_pattern(r"./\{foo,bar}.txt"));
        assert!(!is_glob_pattern("./foo[ab.txt"));
        assert!(!is_glob_pattern("./foo{bar}.txt"));
        assert!(!is_glob_pattern("./foo{bar,baz.txt"));
        assert!(!is_glob_pattern(r"./${foo,bar}.txt"));
    }

    #[test]
    fn extglob_match_exact_at() {
        assert!(extglob_match("@(foo|bar)", "foo"));
        assert!(extglob_match("@(foo|bar)", "bar"));
        assert!(!extglob_match("@(foo|bar)", "baz"));
        assert!(!extglob_match("@(foo|bar)", "foobar"));
        assert!(extglob_match("test_@(a|b|c).rs", "test_a.rs"));
        assert!(extglob_match("test_@(a|b|c).rs", "test_b.rs"));
        assert!(!extglob_match("test_@(a|b|c).rs", "test_d.rs"));
    }

    #[test]
    fn extglob_match_zero_or_one_question() {
        assert!(extglob_match("?(foo|bar)baz", "baz"));
        assert!(extglob_match("?(foo|bar)baz", "foobaz"));
        assert!(extglob_match("?(foo|bar)baz", "barbaz"));
        assert!(!extglob_match("?(foo|bar)baz", "quxbaz"));
        assert!(!extglob_match("?(foo|bar)baz", "foobarbaz"));
    }

    #[test]
    fn extglob_match_zero_or_more_star() {
        assert!(extglob_match("*(foo|bar)baz", "baz"));
        assert!(extglob_match("*(foo|bar)baz", "foobaz"));
        assert!(extglob_match("*(foo|bar)baz", "barbaz"));
        assert!(extglob_match("*(foo|bar)baz", "foobarbaz"));
        assert!(extglob_match("*(foo|bar)baz", "barfoofoobaz"));
        assert!(!extglob_match("*(foo|bar)baz", "otherbaz"));
    }

    #[test]
    fn extglob_match_one_or_more_plus() {
        assert!(!extglob_match("+(foo|bar)baz", "baz"));
        assert!(extglob_match("+(foo|bar)baz", "foobaz"));
        assert!(extglob_match("+(foo|bar)baz", "barbaz"));
        assert!(extglob_match("+(foo|bar)baz", "foobarbaz"));
        assert!(extglob_match("+(foo|bar)baz", "barfoofoobaz"));
        assert!(!extglob_match("+(foo|bar)baz", "otherbaz"));
        assert!(extglob_match("+([0-9])", "12345"));
        assert!(!extglob_match("+([0-9])", "123a45"));
    }

    #[test]
    fn extglob_match_negation_bang() {
        assert!(extglob_match("!(foo|bar)", "baz"));
        assert!(extglob_match("!(foo|bar)", "qux"));
        assert!(!extglob_match("!(foo|bar)", "foo"));
        assert!(!extglob_match("!(foo|bar)", "bar"));
        assert!(extglob_match("!(*.rs)", "Cargo.toml"));
        assert!(extglob_match("!(*.rs)", "README.md"));
        assert!(!extglob_match("!(*.rs)", "lib.rs"));
        assert!(!extglob_match("!(*.rs)", "main.rs"));
    }

    #[test]
    fn extglob_match_nested() {
        assert!(extglob_match("@(a|b*(c|d))", "a"));
        assert!(extglob_match("@(a|b*(c|d))", "b"));
        assert!(extglob_match("@(a|b*(c|d))", "bc"));
        assert!(extglob_match("@(a|b*(c|d))", "bccd"));
        assert!(!extglob_match("@(a|b*(c|d))", "bce"));
        assert!(extglob_match("!(*.@(jpg|png))", "file.gif"));
        assert!(!extglob_match("!(*.@(jpg|png))", "file.jpg"));
        assert!(!extglob_match("!(*.@(jpg|png))", "file.png"));
    }

    #[test]
    fn extglob_match_bracket_classes_and_posix() {
        assert!(extglob_match("[a-z]", "m"));
        assert!(!extglob_match("[a-z]", "M"));
        assert!(extglob_match("[!a-z]", "M"));
        assert!(extglob_match("[^a-z]", "M"));
        assert!(extglob_match("[[:digit:]]", "5"));
        assert!(!extglob_match("[[:digit:]]", "a"));
        assert!(extglob_match("[[:alpha:]]", "Z"));
        assert!(extglob_match("[[:alnum:]]", "9"));
        assert!(extglob_match("[[:alnum:]]", "k"));
        assert!(extglob_match("[[:xdigit:]]", "f"));
        assert!(extglob_match("[[:xdigit:]]", "A"));
        assert!(!extglob_match("[[:xdigit:]]", "g"));
    }

    #[test]
    fn extglob_match_escapes() {
        assert!(extglob_match(r"\*(foo)", "*(foo)"));
        assert!(!extglob_match(r"\*(foo)", "foofoo"));
        assert!(extglob_match(r"\@(a\|b)", "@(a|b)"));
        assert!(extglob_match(r"\[abc\]", "[abc]"));
    }

    #[test]
    fn extglob_match_empty_and_multiple_alternatives() {
        assert!(extglob_match("@(|foo)", ""));
        assert!(extglob_match("@(|foo)", "foo"));
        assert!(!extglob_match("@(|foo)", "bar"));
        assert!(extglob_match("?(a|b)@(c|d)", "ac"));
        assert!(extglob_match("?(a|b)@(c|d)", "c"));
        assert!(extglob_match("?(a|b)@(c|d)", "bd"));
        assert!(!extglob_match("?(a|b)@(c|d)", "a"));
    }

    #[test]
    fn glob_expand_relative_paths() {
        let matches = glob_expand("src/lib.rs", false);
        assert_eq!(matches, vec![PathBuf::from("src/lib.rs")]);

        let matches = glob_expand("src/@(lib|globbing).rs", false);
        assert_eq!(
            matches,
            vec![
                PathBuf::from("src/globbing.rs"),
                PathBuf::from("src/lib.rs"),
            ]
        );
    }

    #[test]
    fn glob_pattern_no_braces() {
        let e = make_expansion("/tmp/foo", "bar*");
        assert_eq!(e.glob_pattern(), vec!["/tmp/foo/bar*".to_string()]);
    }

    #[test]
    fn glob_pattern_single_brace_in_rhs() {
        let e = make_expansion("/tmp/foo", "bar*{A,C}");
        assert_eq!(
            e.glob_pattern(),
            vec!["/tmp/foo/bar*A".to_string(), "/tmp/foo/bar*C".to_string()],
        );
    }

    #[test]
    fn glob_pattern_cartesian_product_two_braces() {
        let e = make_expansion("/tmp/example_braces", "foo*{1,3}/bar*{A,C}");
        assert_eq!(
            e.glob_pattern(),
            vec![
                "/tmp/example_braces/foo*1/bar*A".to_string(),
                "/tmp/example_braces/foo*1/bar*C".to_string(),
                "/tmp/example_braces/foo*3/bar*A".to_string(),
                "/tmp/example_braces/foo*3/bar*C".to_string(),
            ],
        );
    }

    #[test]
    fn glob_pattern_three_alternatives() {
        let e = make_expansion("/tmp/x", "{a,b,c}.txt");
        assert_eq!(
            e.glob_pattern(),
            vec![
                "/tmp/x/a.txt".to_string(),
                "/tmp/x/b.txt".to_string(),
                "/tmp/x/c.txt".to_string(),
            ],
        );
    }

    #[test]
    fn glob_pattern_brace_without_comma_is_literal() {
        let e = make_expansion("/tmp/x", "{single}");
        assert_eq!(e.glob_pattern(), vec!["/tmp/x/{single}".to_string()]);
    }

    #[test]
    fn glob_pattern_nested_braces() {
        let e = make_expansion("/tmp/x", "{a,b{c,d}}");
        assert_eq!(
            e.glob_pattern(),
            vec![
                "/tmp/x/a".to_string(),
                "/tmp/x/bc".to_string(),
                "/tmp/x/bd".to_string(),
            ],
        );
    }

    #[test]
    fn glob_pattern_unmatched_brace_left_alone() {
        let e = make_expansion("/tmp/x", "foo{bar");
        assert_eq!(e.glob_pattern(), vec!["/tmp/x/foo{bar".to_string()]);
    }

    #[test]
    fn glob_pattern_brace_in_expanded_prefix() {
        let e = make_expansion("/tmp/{a,b}", "x*");
        assert_eq!(
            e.glob_pattern(),
            vec!["/tmp/a/x*".to_string(), "/tmp/b/x*".to_string()],
        );
    }

    #[test]
    fn glob_pattern_handles_root_prefix() {
        let e = make_expansion("/", "foo*");
        assert_eq!(e.glob_pattern(), vec!["/foo*".to_string()]);
    }

    #[test]
    fn expand_braces_no_braces() {
        assert_eq!(expand_braces("plain"), vec!["plain".to_string()]);
    }

    #[test]
    fn expand_braces_empty_alternative() {
        assert_eq!(
            expand_braces("x{,foo}y"),
            vec!["xy".to_string(), "xfooy".to_string()],
        );
    }
}
