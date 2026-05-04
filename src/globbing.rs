use crate::bash_funcs::{self, QuoteType};

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
    let mut escaped = false;
    let mut quote = None;
    let mut prev_char = None;

    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            prev_char = Some(c);
            continue;
        }

        if c == '\\' {
            escaped = true;
            prev_char = Some(c);
            continue;
        }

        if c == '\'' || c == '"' {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
            prev_char = Some(c);
            continue;
        }

        if quote.is_some() {
            prev_char = Some(c);
            continue;
        }

        match c {
            '*' | '?' => return Some(i),
            '[' if has_unescaped_closing_bracket(&s[i + c.len_utf8()..]) => return Some(i),
            '{' if prev_char != Some('$')
                && has_unescaped_brace_expansion(&s[i + c.len_utf8()..]) =>
            {
                return Some(i);
            }
            _ => {}
        }

        prev_char = Some(c);
    }

    None
}

fn has_unescaped_closing_bracket(s: &str) -> bool {
    let mut escaped = false;

    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            ']' => return true,
            _ => {}
        }
    }

    false
}

fn has_unescaped_brace_expansion(s: &str) -> bool {
    let mut escaped = false;
    let mut depth = 0;
    let mut has_comma = false;
    let mut has_sequence = false;

    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' if depth == 0 => return has_comma || has_sequence,
            '}' => depth -= 1,
            ',' if depth == 0 => has_comma = true,
            '.' if depth == 0 && s[i + c.len_utf8()..].starts_with('.') => {
                has_sequence = true;
            }
            _ => {}
        }
    }

    false
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
        let expanded_prefix = bash_funcs::fully_expand_path(&raw_prefix);

        let rhs_pattern = bash_funcs::dequoting_function_rust(&rhs_pattern);

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
    /// expands to four patterns).  When the pattern contains no brace
    /// alternatives, the returned vector has a single element.
    pub(crate) fn glob_pattern(&self) -> Vec<String> {
        let combined = join_path_parts(&self.expanded_prefix, &self.rhs_pattern);
        expand_braces(&combined)
    }

    pub(crate) fn wants_hidden(&self) -> bool {
        self.rhs_pattern.starts_with('.') && !self.rhs_pattern.starts_with("./")
    }

    pub(crate) fn convert_expanded_match_to_unexpanded(
        &self,
        expanded_match: &str,
        quote_type: Option<QuoteType>,
    ) -> (String, String) {
        let expected_prefix = if self.expanded_prefix.ends_with('/') {
            self.expanded_prefix.clone()
        } else {
            format!("{}/", self.expanded_prefix)
        };

        if let Some(rhs) = expanded_match.strip_prefix(&expected_prefix) {
            let quoted_rhs = bash_funcs::quoting_function_rust(
                rhs,
                quote_type.unwrap_or_default(),
                false,
                false,
            );
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
/// drive `glob::glob` from a pattern such as `foo*{1,3}/bar*{A,C}`.
///
/// When `pattern` contains no expandable braces, the returned vector contains
/// `pattern` unchanged.
fn expand_braces(pattern: &str) -> Vec<String> {
    // Find the first unescaped '{' that has a matching '}' at the same nesting
    // level and at least one top-level comma between them.
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
            // Recursively expand the suffix (and any further braces in it)
            // for every alternative, then expand each alternative itself
            // (in case it contained nested braces that we left alone above).
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
        // Mirrors the integration test pattern: `$PWD/foo*{1,3}/bar*{A,C}`.
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
        // `{single}` is not a brace alternation in bash — it's left intact.
        let e = make_expansion("/tmp/x", "{single}");
        assert_eq!(e.glob_pattern(), vec!["/tmp/x/{single}".to_string()]);
    }

    #[test]
    fn glob_pattern_nested_braces() {
        // `{a,b{c,d}}` -> a, bc, bd
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
        // Brace expansion should also kick in when the brace lives in the
        // expanded prefix portion.
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
        // `{,foo}` -> "" and "foo"
        assert_eq!(
            expand_braces("x{,foo}y"),
            vec!["xy".to_string(), "xfooy".to_string()],
        );
    }
}
