use std::ops::Range;

// QuoteType can be in the middle of a word (i.e. backslash)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote,
    #[default]
    Backslash,
}

impl QuoteType {
    pub fn from_char(c: char) -> Option<QuoteType> {
        match c {
            '\'' => Some(QuoteType::SingleQuote),
            '"' => Some(QuoteType::DoubleQuote),
            '\\' => Some(QuoteType::Backslash),
            _ => None,
        }
    }

    pub fn into_byte(self) -> u8 {
        match self {
            QuoteType::SingleQuote => b'\'',
            QuoteType::DoubleQuote => b'"',
            QuoteType::Backslash => b'\\',
        }
    }
}

pub const DOUBLE_QUOTE_SPECIAL_CHARS: &[char] = &['$', '`', '"', '\\', '!', '\n'];
pub const BACKSLASH_SPECIAL_CHARS: &[char] = &[
    ' ', '\t', '\n', '\\', '"', '\'', '!', '$', '&', '(', ')', '*', ';', '<', '>', '?', '[', ']',
    '^', '`', '{', '|', '}',
];

/* Quote a filename using double quotes, single quotes, or backslashes
depending on the value of completion_quoting_style.  If we're
completing using backslashes, we need to quote some additional
characters (those that readline treats as word breaks), so we call
quote_word_break_chars on the result.  This returns newly-allocated
memory. */
// static char * bash_quote_filename (char *s, int rtype, char *qcp)
// TODO: handle edge cases that bash_quote_filename handles
pub fn quoting_function_rust(
    s: &str,
    quote_type: QuoteType,
    opening_quote: bool,
    closing_quote: bool,
) -> String {
    match quote_type {
        QuoteType::SingleQuote => {
            let mut quoted = s.replace('\'', "'\\''");
            if opening_quote {
                quoted = format!("'{}", quoted);
            }
            if closing_quote {
                quoted.push('\'');
            }
            quoted
        }
        QuoteType::DoubleQuote => {
            let escaped: String = s
                .chars()
                .map(|c| {
                    if DOUBLE_QUOTE_SPECIAL_CHARS.contains(&c) {
                        format!("\\{}", c)
                    } else {
                        c.to_string()
                    }
                })
                .collect();

            let mut quoted = if opening_quote {
                format!("\"{}", escaped)
            } else {
                escaped
            };
            if closing_quote {
                quoted.push('"');
            }
            quoted
        }
        QuoteType::Backslash => s
            .chars()
            .map(|c| {
                if c.is_whitespace() || BACKSLASH_SPECIAL_CHARS.contains(&c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            })
            .collect(),
    }
}

/* Filename quoting for completion. */
/* A function to strip unquoted quote characters (single quotes, double
quotes, and backslashes).  It allows single quotes to appear
within double quotes, and vice versa.  It should be smarter. */
// static char *bash_dequote_filename (char *text, int quote_char)
pub fn dequoting_function_rust(s: &str) -> String {
    let mut res = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next_char) = chars.next() {
                    res.push(next_char);
                }
            }
            '\'' => {
                for next_char in chars.by_ref() {
                    if next_char == '\'' {
                        break;
                    }
                    res.push(next_char);
                }
            }
            '"' => {
                while let Some(next_char) = chars.next() {
                    if next_char == '"' {
                        break;
                    }
                    if next_char == '\\' {
                        if let Some(escaped_char) = chars.next() {
                            res.push(escaped_char);
                        }
                    } else {
                        res.push(next_char);
                    }
                }
            }
            _ => res.push(c),
        }
    }
    res
}

// This function
//    returns the opening quote character if we found an unclosed quoted
//    substring, '\0' otherwise.  FP, if non-null, is set to a value saying
//    which (shell-like) quote characters we found (single quote, double
//    quote, or backslash) anywhere in the string.  DP, if non-null, is set to
//    the value of the delimiter character that caused a word break. */
// It sets fp to  a bitfield  but no one ever reads that bitfield so we can ignore it for now
// char _rl_find_completion_word (int *fp, int *dp)

pub fn find_quote_type(s: &str) -> Option<QuoteType> {
    if s.starts_with('\"') {
        return Some(QuoteType::DoubleQuote);
    } else if s.starts_with('\'') {
        return Some(QuoteType::SingleQuote);
    } else {
        // Check for odd number of consecutive backslashes
        let mut backslash_count = 0;
        let mut max_consecutive_backslashes = 0;

        for c in s.chars() {
            if c == '\\' {
                backslash_count += 1;
            } else if backslash_count > 0 {
                max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
                backslash_count = 0;
            }
        }
        // Handle case where string ends with backslashes
        if backslash_count > 0 {
            max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
        }

        if max_consecutive_backslashes > 0 && max_consecutive_backslashes % 2 == 1 {
            return Some(QuoteType::Backslash);
        }
    }
    None
}

/// The kind of string whose escape sequences [`escape_sequence_ranges`] looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeContext {
    /// `"..."`: a backslash only escapes `$`, `` ` ``, `"` and `\`. Any other backslash is
    /// kept as it is, so `"\n"` is a backslash followed by `n`.
    DoubleQuotes,
    /// `$'...'`: the ANSI-C escape sequences, such as `\n`, `\x41`, `\uHHHH` and `\cA`.
    AnsiC,
}

/// Byte ranges of the escape sequences in `text`, the content of a `"..."` or `$'...'`
/// string without its quotes. A backslash that does not start an escape sequence, like the
/// one in `"\n"`, is not part of any range.
pub fn escape_sequence_ranges(text: &str, context: EscapeContext) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    while let Some(offset) = text[pos..].find('\\') {
        let start = pos + offset;
        let after_backslash = &text[start + 1..];
        let len = match context {
            EscapeContext::DoubleQuotes => double_quote_escape_len(after_backslash),
            EscapeContext::AnsiC => ansi_c_escape_len(after_backslash),
        };
        match len {
            Some(len) => {
                let end = start + 1 + len;
                ranges.push(start..end);
                pos = end;
            }
            None => pos = start + 1,
        }
    }
    ranges
}

/// The length in bytes of the escape sequence after a backslash in `"..."`, if there is one.
fn double_quote_escape_len(after_backslash: &str) -> Option<usize> {
    matches!(after_backslash.chars().next(), Some('$' | '`' | '"' | '\\')).then_some(1)
}

/// The length in bytes of the ANSI-C escape sequence after a backslash in `$'...'`, if there
/// is one. These are the escape sequences bash documents for `$'...'`; bash keeps the
/// backslash of anything else, such as `\z` or a `\x` without hex digits.
fn ansi_c_escape_len(after_backslash: &str) -> Option<usize> {
    let mut chars = after_backslash.chars();
    let first = chars.next()?;
    // The digits after `\x`, `\u`, `\U` or an octal `\N`, up to `max` of them. They are
    // ASCII, so their count is also their length in bytes.
    let digits = |max: usize, is_digit: fn(char) -> bool| {
        let count = chars
            .as_str()
            .chars()
            .take(max)
            .take_while(|&c| is_digit(c))
            .count();
        (count > 0).then_some(count)
    };
    match first {
        'a' | 'b' | 'e' | 'E' | 'f' | 'n' | 'r' | 't' | 'v' | '\\' | '\'' | '"' | '?' => Some(1),
        // `\N`, `\NN` or `\NNN`: the first octal digit and up to two more.
        '0'..='7' => Some(1 + digits(2, |c| c.is_digit(8)).unwrap_or(0)),
        'x' => digits(2, |c| c.is_ascii_hexdigit()).map(|n| 1 + n),
        'u' => digits(4, |c| c.is_ascii_hexdigit()).map(|n| 1 + n),
        'U' => digits(8, |c| c.is_ascii_hexdigit()).map(|n| 1 + n),
        // `\cX` is a control character. `\c\\` uses both backslashes for one `\`.
        'c' => {
            let control = chars.next()?;
            let doubled_backslash = control == '\\' && chars.next() == Some('\\');
            Some(1 + control.len_utf8() + usize::from(doubled_backslash))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_function() {
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::Backslash, true, true),
            r#"qwe\ asd"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::DoubleQuote, true, true),
            r#""qwe asd""#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::SingleQuote, true, true),
            r#"'qwe asd'"#
        );
    }

    #[test]
    fn test_quote_function_harder() {
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::Backslash, true, true),
            r#"qwe\"asdf"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::DoubleQuote, true, true),
            r#""qwe\"asdf""#
        );
    }

    #[test]
    fn test_quote_function_backslash_special_chars() {
        for &c in BACKSLASH_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected = format!("a\\{}b", c);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::Backslash, true, true),
                expected
            );
        }
    }

    #[test]
    fn test_quote_function_double_quote_special_chars() {
        for &c in DOUBLE_QUOTE_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected_inner = format!("a\\{}b", c);
            let expected = format!("\"{}\"", expected_inner);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::DoubleQuote, true, true),
                expected
            );
        }
    }

    #[test]
    fn test_dequoting_function() {
        assert_eq!(dequoting_function_rust(r#"qwe\ asd"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#""qwe asd""#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"'qwe asd'"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"abc"#), r#"abc"#);
    }

    #[test]
    fn test_dequoting_function_harder() {
        assert_eq!(dequoting_function_rust(r#"qwe\"asdf"#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""qwe\"asdf""#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""""#), r#""#);
    }

    #[test]
    fn test_find_quotes() {
        assert_eq!(
            find_quote_type(r#""qwe asdf"#),
            Some(QuoteType::DoubleQuote)
        );
        assert_eq!(
            find_quote_type(r#"'qwe asdf"#),
            Some(QuoteType::SingleQuote)
        );
        assert_eq!(find_quote_type(r#"qwe\ asdf"#), Some(QuoteType::Backslash));
        assert_eq!(find_quote_type(r#"qwe asdf"#), None);
        assert_eq!(find_quote_type(r#"qwe\\asdf"#), None);
    }

    /// The escape sequences `escape_sequence_ranges` finds in `text`, as slices of it.
    fn marked(text: &str, context: EscapeContext) -> Vec<&str> {
        escape_sequence_ranges(text, context)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn test_escape_sequences_in_double_quotes() {
        // The text of `echo "Price: \$5, a \"quote\", a backslash \\, a newline \n"`.
        let text = r#"Price: \$5, a \"quote\", a backslash \\, a newline \n"#;
        assert_eq!(
            marked(text, EscapeContext::DoubleQuotes),
            vec![r"\$", r#"\""#, r#"\""#, r"\\"]
        );
        assert_eq!(
            marked(r"\`date\`", EscapeContext::DoubleQuotes),
            vec![r"\`", r"\`"]
        );

        // Bash keeps any other backslash inside double quotes.
        for text in [r"\n", r"\t", r"\x41", r"\!", r"\'", r"a\", r"\"] {
            assert!(marked(text, EscapeContext::DoubleQuotes).is_empty());
        }

        // An escaped backslash does not escape what follows it.
        assert_eq!(marked(r"\\n", EscapeContext::DoubleQuotes), vec![r"\\"]);
        assert_eq!(
            marked(r"\\\$", EscapeContext::DoubleQuotes),
            vec![r"\\", r"\$"]
        );

        // The ranges are byte ranges.
        assert_eq!(
            escape_sequence_ranges("é\\$ü", EscapeContext::DoubleQuotes),
            vec![2..4]
        );
    }

    #[test]
    fn test_escape_sequences_in_ansi_c_strings() {
        // The text of `echo $'Line one\nLine two\tTabbed \x41 \u00e9'`.
        let text = "Line one\\nLine two\\tTabbed \\x41 \\u00e9";
        assert_eq!(
            marked(text, EscapeContext::AnsiC),
            vec![r"\n", r"\t", r"\x41", "\\u00e9"]
        );

        for escape in [
            r"\a", r"\b", r"\e", r"\E", r"\f", r"\n", r"\r", r"\t", r"\v", r"\\", r"\'", r#"\""#,
            r"\?",
        ] {
            assert_eq!(marked(escape, EscapeContext::AnsiC), vec![escape]);
        }

        // An escaped backslash does not escape what follows it.
        assert_eq!(marked(r"\\n", EscapeContext::AnsiC), vec![r"\\"]);

        // The ranges are byte ranges.
        assert_eq!(
            escape_sequence_ranges("é\\nü", EscapeContext::AnsiC),
            vec![2..4]
        );
    }

    #[test]
    fn test_ansi_c_escape_sequence_lengths() {
        // (text, the escape sequences in it). The digits stop at their maximum count, so the
        // last digit of `\1012`, `\x414`, `\u00e9a` and `\U0001F6000` is plain text.
        let cases: &[(&str, &[&str])] = &[
            (r"\0", &[r"\0"]),
            (r"\7", &[r"\7"]),
            (r"\101", &[r"\101"]),
            (r"\1012", &[r"\101"]),
            (r"\x4", &[r"\x4"]),
            (r"\x41", &[r"\x41"]),
            (r"\x414", &[r"\x41"]),
            (r"\ue", &[r"\ue"]),
            ("\\u00e9", &["\\u00e9"]),
            ("\\u00e9a", &["\\u00e9"]),
            (r"\U1F600", &[r"\U1F600"]),
            (r"\U0001F600", &[r"\U0001F600"]),
            (r"\U0001F6000", &[r"\U0001F600"]),
            (r"\cA", &[r"\cA"]),
            (r"\c\\", &[r"\c\\"]),
            (r"\c\\x", &[r"\c\\"]),
            // Bash keeps the backslash of these.
            (r"\8", &[]),
            (r"\x", &[]),
            (r"\xg", &[]),
            (r"\u", &[]),
            (r"\U", &[]),
            (r"\z", &[]),
            (r"\c", &[]),
            (r"\", &[]),
        ];
        for (text, expected) in cases {
            assert_eq!(marked(text, EscapeContext::AnsiC), *expected);
        }

        // `\c` takes a whole character, even a multi-byte one.
        assert_eq!(marked("\\cé", EscapeContext::AnsiC), vec!["\\cé"]);
    }
}
