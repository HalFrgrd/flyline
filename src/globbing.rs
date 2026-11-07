use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::grammar::{DParser, QuoteType, TokenKind, quoting_function_rust};
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
    split_glob_pattern_with_wildcard(s, None)
}

fn split_glob_pattern_with_wildcard(
    s: &str,
    prefix_wildcard: Option<usize>,
) -> GlobPatternSplit<'_> {
    let first_glob_pos = match (first_glob_pos(s), prefix_wildcard) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
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
    /// This is literal: [`split_glob_pattern`] stops before the first glob character,
    /// so it is never globbed again (cwd, `~` and variable values may contain `[`, `{`, `*`, ...).
    /// it might be empty: e.g. when the current directory cannot be determined.
    expanded_prefix: String,
    /// The part of the pattern after the separating`/`— the glob portion
    /// (e.g. `baz*` or `*/*.txt`). Glob metacharacters the user quoted or
    /// escaped are kept escaped, e.g. `fi\[1\]*`.
    rhs_pattern: String,
}

impl PathPatternExpansion {
    /// `prefix_wildcard` is the byte index of the synthetic prefix-matching `*`
    /// appended by the caller. It stays a wildcard even inside an unclosed quote.
    pub(crate) fn new(pattern: &str, prefix_wildcard: Option<usize>) -> Self {
        let split = split_glob_pattern_with_wildcard(pattern, prefix_wildcard);
        let raw_prefix = split.raw_prefix.to_string();
        let expanded_prefix = shell::backend().expand_path(&raw_prefix);
        let rhs_pattern = dequote_glob_pattern(pattern, &split, prefix_wildcard);

        PathPatternExpansion {
            raw_prefix,
            expanded_prefix,
            rhs_pattern,
        }
    }

    /// The literal directory the glob patterns are relative to.
    pub(crate) fn base_dir(&self) -> &Path {
        Path::new(&self.expanded_prefix)
    }

    /// Build the glob pattern(s) matched inside [`Self::base_dir`].
    ///
    /// The returned vector contains the cartesian product of any brace
    /// expansions present in the pattern (e.g. `foo*{1,3}/bar*{A,C}`
    /// expands to four patterns). When the pattern contains no brace
    /// alternatives, the returned vector has a single element.
    pub(crate) fn glob_patterns(&self) -> Vec<String> {
        expand_braces(&self.rhs_pattern)
    }

    /// Perform lazy segment-by-segment filesystem expansion across all brace-expanded patterns.
    pub(crate) fn expand_iter(&self) -> impl Iterator<Item = PathBuf> + '_ {
        let base_dir = self.base_dir();
        let wants_hidden = self.wants_hidden();
        self.glob_patterns()
            .into_iter()
            .flat_map(move |pat| GlobIterator::new_at(base_dir, &pat, wants_hidden))
    }

    /// Perform segment-by-segment filesystem expansion across all brace-expanded patterns.
    #[cfg(test)]
    pub(crate) fn expand(&self) -> Vec<PathBuf> {
        let mut results: Vec<PathBuf> = self.expand_iter().collect();
        results.sort();
        results.dedup();
        results
    }

    pub(crate) fn wants_hidden(&self) -> bool {
        self.rhs_pattern.starts_with('.') && !self.rhs_pattern.starts_with("./")
    }

    pub(crate) fn static_prefix(&self) -> Option<String> {
        if self.raw_prefix.is_empty() {
            None
        } else if self.raw_prefix == "/" {
            Some("/".to_string())
        } else {
            Some(format!("{}/", self.raw_prefix))
        }
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

/// Characters interpreted by [`extglob_match`] or [`expand_braces`].
fn is_glob_meta(c: char) -> bool {
    matches!(
        c,
        '*' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '|' | '@' | '+' | '!' | '\\'
    )
}

#[derive(Default, Clone, Copy)]
enum QuoteState {
    #[default]
    Unquoted,
    Single,
    Double,
}

#[derive(Default)]
struct GlobDequoter {
    quote: QuoteState,
    escaped: bool,
    out: String,
}

impl GlobDequoter {
    fn feed(&mut self, c: char, is_prefix_wildcard: bool) {
        if is_prefix_wildcard {
            self.out.push(c);
            return;
        }
        if self.escaped {
            self.escaped = false;
            self.push_literal(c);
            return;
        }
        match self.quote {
            QuoteState::Single => {
                if c == '\'' {
                    self.quote = QuoteState::Unquoted;
                } else {
                    self.push_literal(c);
                }
            }
            QuoteState::Double => match c {
                '"' => self.quote = QuoteState::Unquoted,
                '\\' => self.escaped = true,
                _ => self.push_literal(c),
            },
            QuoteState::Unquoted => match c {
                '\\' => self.escaped = true,
                '\'' => self.quote = QuoteState::Single,
                '"' => self.quote = QuoteState::Double,
                _ => self.out.push(c),
            },
        }
    }

    fn push_literal(&mut self, c: char) {
        if is_glob_meta(c) {
            self.out.push('\\');
        }
        self.out.push(c);
    }
}

/// Dequote a user-typed glob pattern while keeping the metacharacters that the
/// user quoted or backslash-escaped escaped, so [`extglob_match`] and
/// [`expand_braces`] treat them literally.
///
/// The quote state at the [`split_glob_pattern`] boundary is carried over from
/// the prefix, since the pattern is a slice of one shell word. `prefix_wildcard`
/// marks the synthetic prefix-matching `*`, which is not user text and therefore
/// never literal.
fn dequote_glob_pattern(
    pattern: &str,
    split: &GlobPatternSplit<'_>,
    prefix_wildcard: Option<usize>,
) -> String {
    let rhs_start = pattern.len() - split.rhs_pattern.len();
    let mut dequoter = GlobDequoter::default();
    for (idx, c) in pattern.char_indices() {
        if idx == rhs_start {
            dequoter.out.clear();
        }
        dequoter.feed(c, prefix_wildcard == Some(idx));
    }
    dequoter.out
}

/// Remove the escapes [`dequote_glob_pattern`] added for literal metacharacters.
fn unescape_glob_segment(segment: &str) -> Cow<'_, str> {
    if !segment.contains('\\') {
        return Cow::Borrowed(segment);
    }

    let mut out = String::with_capacity(segment.len());
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
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
    let Some(first_char) = pattern.chars().next() else {
        return text.is_empty();
    };

    // 1. Escaped character
    if first_char == '\\' {
        let rest = &pattern[1..];
        let Some(escaped_char) = rest.chars().next() else {
            return text == "\\";
        };
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
        let Some(text_char) = text.chars().next() else {
            return false;
        };
        return extglob_match_inner(&pattern[1..], &text[text_char.len_utf8()..]);
    }

    // 5. Bracket expression: [...]
    if first_char == '['
        && let Some((end_idx, is_negated, spec)) = parse_bracket_class(pattern)
    {
        let Some(text_char) = text.chars().next() else {
            return false;
        };
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
struct StackFrame {
    base_dir: PathBuf,
    seg_idx: usize,
    read_dir: Option<std::fs::ReadDir>,
}

pub struct GlobIterator {
    segments: Vec<String>,
    wants_hidden: bool,
    stack: Vec<StackFrame>,
    root_to_yield: Option<PathBuf>,
}

impl GlobIterator {
    /// Expand `pattern` relative to the process' current directory.
    #[cfg(test)]
    pub fn new(pattern: &str, wants_hidden: bool) -> Self {
        Self::new_at(Path::new(""), pattern, wants_hidden)
    }

    /// Expand `pattern` relative to `base_dir`, which is literal: it is never
    /// parsed as a glob pattern, whatever characters it contains.
    fn new_at(base_dir: &Path, pattern: &str, wants_hidden: bool) -> Self {
        let (base_dir, pattern) = match pattern.strip_prefix('/') {
            Some(rest) if base_dir.as_os_str().is_empty() => (Path::new("/"), rest),
            _ => (base_dir, pattern),
        };

        let segments: Vec<String> = if pattern.is_empty() {
            Vec::new()
        } else {
            split_path_segments(pattern)
                .into_iter()
                .map(String::from)
                .collect()
        };

        if segments.is_empty() {
            let root_to_yield = (!base_dir.as_os_str().is_empty()).then(|| base_dir.to_path_buf());
            return GlobIterator {
                segments,
                wants_hidden,
                stack: Vec::new(),
                root_to_yield,
            };
        }

        let stack = vec![StackFrame {
            base_dir: base_dir.to_path_buf(),
            seg_idx: 0,
            read_dir: None,
        }];

        GlobIterator {
            segments,
            wants_hidden,
            stack,
            root_to_yield: None,
        }
    }
}

impl Iterator for GlobIterator {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(root) = self.root_to_yield.take() {
            return Some(root);
        }

        while let Some(frame) = self.stack.last_mut() {
            if frame.seg_idx >= self.segments.len() {
                self.stack.pop();
                continue;
            }

            let segment = &self.segments[frame.seg_idx];
            let is_last = frame.seg_idx == self.segments.len() - 1;

            if segment.is_empty() {
                frame.seg_idx += 1;
                continue;
            }

            let is_glob_seg = has_glob_or_extglob(segment);

            if !is_glob_seg {
                let base_dir = frame.base_dir.clone();
                let seg_idx = frame.seg_idx;
                self.stack.pop();

                let segment = unescape_glob_segment(segment);
                let candidate = if base_dir.as_os_str().is_empty() {
                    PathBuf::from(segment.as_ref())
                } else if base_dir == Path::new("/") {
                    PathBuf::from(format!("/{}", segment))
                } else {
                    base_dir.join(segment.as_ref())
                };

                if is_last {
                    if candidate.exists() || candidate.is_symlink() {
                        return Some(candidate);
                    }
                } else if candidate.is_dir() {
                    self.stack.push(StackFrame {
                        base_dir: candidate,
                        seg_idx: seg_idx + 1,
                        read_dir: None,
                    });
                }
                continue;
            }

            // Glob segment
            if frame.read_dir.is_none() {
                let dir_to_read = if frame.base_dir.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    frame.base_dir.as_path()
                };

                match std::fs::read_dir(dir_to_read) {
                    Ok(rd) => frame.read_dir = Some(rd),
                    Err(_) => {
                        self.stack.pop();
                        continue;
                    }
                }
            }

            let seg_wants_hidden = self.wants_hidden || segment.starts_with('.');
            let mut pushed_next = false;
            let mut result_item = None;

            if let Some(read_dir) = frame.read_dir.as_mut() {
                for entry in read_dir.by_ref().filter_map(Result::ok) {
                    let file_name = entry.file_name();
                    let file_name_str = file_name.to_string_lossy();

                    if file_name_str == "." || file_name_str == ".." {
                        continue;
                    }

                    if !seg_wants_hidden && file_name_str.starts_with('.') {
                        continue;
                    }

                    if extglob_match(segment, &file_name_str) {
                        let full_path = if frame.base_dir.as_os_str().is_empty() {
                            PathBuf::from(file_name_str.as_ref())
                        } else if frame.base_dir == Path::new("/") {
                            PathBuf::from(format!("/{}", file_name_str))
                        } else {
                            frame.base_dir.join(file_name_str.as_ref())
                        };

                        if is_last {
                            result_item = Some(full_path);
                            break;
                        } else if entry.path().is_dir() {
                            let next_seg = frame.seg_idx + 1;
                            self.stack.push(StackFrame {
                                base_dir: full_path,
                                seg_idx: next_seg,
                                read_dir: None,
                            });
                            pushed_next = true;
                            break;
                        }
                    }
                }
            }

            if let Some(item) = result_item {
                return Some(item);
            }

            if !pushed_next {
                self.stack.pop();
            }
        }

        None
    }
}

#[cfg(test)]
pub fn glob_expand_iter(pattern: &str, wants_hidden: bool) -> GlobIterator {
    GlobIterator::new(pattern, wants_hidden)
}

#[cfg(test)]
pub fn glob_expand(pattern: &str, wants_hidden: bool) -> Vec<PathBuf> {
    let mut results: Vec<PathBuf> = glob_expand_iter(pattern, wants_hidden).collect();
    results.sort();
    results.dedup();
    results
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
    /// at link time. Used to unit-test `base_dir`/`glob_patterns`/expansion
    /// in isolation.
    fn make_expansion(expanded_prefix: &str, rhs_pattern: &str) -> PathPatternExpansion {
        PathPatternExpansion {
            raw_prefix: String::new(),
            expanded_prefix: expanded_prefix.to_string(),
            rhs_pattern: rhs_pattern.to_string(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("flyline_glob_{}_{}", name, rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
    fn glob_expand_iter_short_circuits() {
        let mut iter = glob_expand_iter("src/**/*.rs", false);
        let first_two: Vec<_> = iter.by_ref().take(2).collect();
        assert_eq!(first_two.len(), 2);
    }

    #[test]
    fn glob_patterns_no_braces() {
        let e = make_expansion("/tmp/foo", "bar*");
        assert_eq!(e.base_dir(), Path::new("/tmp/foo"));
        assert_eq!(e.glob_patterns(), vec!["bar*".to_string()]);
    }

    #[test]
    fn glob_patterns_single_brace_in_rhs() {
        let e = make_expansion("/tmp/foo", "bar*{A,C}");
        assert_eq!(
            e.glob_patterns(),
            vec!["bar*A".to_string(), "bar*C".to_string()],
        );
    }

    #[test]
    fn glob_patterns_cartesian_product_two_braces() {
        let e = make_expansion("/tmp/example_braces", "foo*{1,3}/bar*{A,C}");
        assert_eq!(
            e.glob_patterns(),
            vec![
                "foo*1/bar*A".to_string(),
                "foo*1/bar*C".to_string(),
                "foo*3/bar*A".to_string(),
                "foo*3/bar*C".to_string(),
            ],
        );
    }

    #[test]
    fn glob_patterns_three_alternatives() {
        let e = make_expansion("/tmp/x", "{a,b,c}.txt");
        assert_eq!(
            e.glob_patterns(),
            vec![
                "a.txt".to_string(),
                "b.txt".to_string(),
                "c.txt".to_string()
            ],
        );
    }

    #[test]
    fn glob_patterns_brace_without_comma_is_literal() {
        let e = make_expansion("/tmp/x", "{single}");
        assert_eq!(e.glob_patterns(), vec!["{single}".to_string()]);
    }

    #[test]
    fn glob_patterns_nested_braces() {
        let e = make_expansion("/tmp/x", "{a,b{c,d}}");
        assert_eq!(
            e.glob_patterns(),
            vec!["a".to_string(), "bc".to_string(), "bd".to_string()],
        );
    }

    #[test]
    fn glob_patterns_unmatched_brace_left_alone() {
        let e = make_expansion("/tmp/x", "foo{bar");
        assert_eq!(e.glob_patterns(), vec!["foo{bar".to_string()]);
    }

    #[test]
    fn glob_patterns_ignore_braces_in_base() {
        let e = make_expansion("/tmp/{a,b}", "x*");
        assert_eq!(e.base_dir(), Path::new("/tmp/{a,b}"));
        assert_eq!(e.glob_patterns(), vec!["x*".to_string()]);
    }

    #[test]
    fn glob_patterns_handles_root_base() {
        let e = make_expansion("/", "foo*");
        assert_eq!(e.base_dir(), Path::new("/"));
        assert_eq!(e.glob_patterns(), vec!["foo*".to_string()]);
    }

    #[test]
    fn base_dir_metacharacters_are_literal() {
        let base = temp_dir("weird [1] {b,c} *d? e\\f 中文");
        std::fs::write(base.join("alpha.txt"), "").unwrap();
        std::fs::write(base.join("beta.md"), "").unwrap();
        let path = base.to_str().unwrap();

        let e = make_expansion(path, "*");
        assert_eq!(
            e.expand(),
            vec![base.join("alpha.txt"), base.join("beta.md")],
        );

        let e = make_expansion(path, "*.txt");
        assert_eq!(e.expand(), vec![base.join("alpha.txt")]);

        let e = make_expansion(path, "al*");
        assert_eq!(e.expand(), vec![base.join("alpha.txt")]);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn escaped_metacharacters_in_pattern_are_literal() {
        let base = temp_dir("escaped_pattern");
        for name in [
            "lit[1].txt",
            "lit{2}.txt",
            "lit*3.txt",
            "lit?4.txt",
            "fix.txt",
        ] {
            std::fs::write(base.join(name), "").unwrap();
        }

        for (pattern, expected) in [
            (r"lit\[1\].txt", "lit[1].txt"),
            (r"lit\{2\}.txt", "lit{2}.txt"),
            (r"lit\*3.txt", "lit*3.txt"),
            (r"lit\?4.txt", "lit?4.txt"),
        ] {
            let e = make_expansion(base.to_str().unwrap(), pattern);
            assert_eq!(e.expand(), vec![base.join(expected)], "pattern: {pattern}");
        }

        // Unescaped metacharacters keep their glob meaning.
        let e = make_expansion(base.to_str().unwrap(), "lit*");
        assert_eq!(e.expand().len(), 4);

        let e = make_expansion(base.to_str().unwrap(), "fi?*");
        assert_eq!(e.expand(), vec![base.join("fix.txt")]);

        std::fs::remove_dir_all(&base).unwrap();
    }

    fn dequote(pattern: &str, prefix_wildcard: Option<usize>) -> String {
        let split = split_glob_pattern_with_wildcard(pattern, prefix_wildcard);
        dequote_glob_pattern(pattern, &split, prefix_wildcard)
    }

    #[test]
    fn split_glob_pattern_stops_before_prefix_wildcard_in_unclosed_quote() {
        // `mycmd "dir [1]/su█b/file.txt`: the synthetic `*` at index 11 is inside
        // an unclosed quote, so `split_glob_pattern_with_wildcard` must split at
        // the `/` before `su*b`, not the `/` after it.
        assert_eq!(
            split_glob_pattern_with_wildcard("\"dir [1]/su*b/file.txt", Some(11)),
            GlobPatternSplit {
                raw_prefix: "\"dir [1]",
                rhs_pattern: "su*b/file.txt",
                has_glob: true,
            }
        );
        assert_eq!(
            dequote("\"dir [1]/su*b/file.txt", Some(11)),
            "su*b/file.txt"
        );
    }

    #[test]
    fn dequote_glob_pattern_preserves_literal_metacharacters() {
        assert_eq!(dequote("fi*", None), "fi*");
        assert_eq!(dequote(r"fi\*", None), r"fi\*");
        assert_eq!(dequote(r"fi\[1\]*", None), r"fi\[1\]*");
        assert_eq!(dequote(r"fi\{a,b\}*", None), r"fi\{a,b\}*");
        assert_eq!(dequote(r"fi\a*", None), "fia*");
        assert_eq!(dequote(r"fi\\*", None), r"fi\\*");
        assert_eq!(dequote("fi[1]*", None), "fi[1]*");
        assert_eq!(dequote("\"fi[1]*\"", None), r"fi\[1\]\*");
        assert_eq!(dequote("'fi{1}?'", None), r"fi\{1\}\?");
    }

    #[test]
    fn dequote_glob_pattern_carries_quote_state_across_split() {
        assert_eq!(dequote("\"fi/*.txt", None), r"\*.txt");
        assert_eq!(dequote("'fi/[1]", None), r"\[1\]");
        assert_eq!(dequote("fi/*.txt", None), "*.txt");
    }

    #[test]
    fn dequote_glob_pattern_keeps_prefix_wildcard_unquoted() {
        // `mycmd "fi` + Tab: the appended `*` is a prefix wildcard, not literal
        // text inside the still-open quote.
        assert_eq!(dequote("\"fi*", Some(3)), "fi*");
        assert_eq!(dequote("\"$HOME/*", Some(7)), "*");
        assert_eq!(dequote("\"fi*le\"", Some(3)), "fi*le");
        // Quoted metacharacters around the wildcard stay literal.
        assert_eq!(dequote("\"fi[1]*", Some(6)), r"fi\[1\]*");
        assert_eq!(dequote("'fi[1]*", Some(6)), r"fi\[1\]*");
    }

    #[test]
    fn unescape_glob_segment_removes_escapes() {
        assert_eq!(unescape_glob_segment("plain"), "plain");
        assert_eq!(unescape_glob_segment(r"li\*[1]"), "li*[1]");
        assert_eq!(unescape_glob_segment(r"a\\b"), r"a\b");
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
