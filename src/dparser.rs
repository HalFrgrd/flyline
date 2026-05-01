use flash::lexer::{Lexer, Token, TokenKind};
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};

pub fn collect_tokens_include_whitespace(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = matches!(token.kind, TokenKind::EOF);
        if is_eof {
            break;
        }
        tokens.push(token);
    }

    tokens
}

pub trait ToInclusiveRange {
    fn to_inclusive(&self) -> RangeInclusive<usize>;
}

impl ToInclusiveRange for Range<usize> {
    fn to_inclusive(&self) -> RangeInclusive<usize> {
        self.start..=self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingAnnotation {
    pub opening_idx: usize,     // index of the opening token in the tokens vector
    pub is_auto_inserted: bool, // true if this closing token was automatically inserted by the editor
}

/// Represents the matched/unmatched state of an opening delimiter token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpeningState {
    /// The opening delimiter has been found but its closing counterpart has not yet been matched.
    Unmatched,
    /// The opening delimiter is matched with a closing token at the given index.
    Matched(usize),
}

/// All annotations that can be applied to a token. Multiple annotations can be present
/// simultaneously (e.g. a token can be both inside double quotes and an env var).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Annotations {
    pub is_inside_single_quotes: bool,
    pub is_inside_double_quotes: bool,
    pub is_env_var: bool,
    pub is_comment: bool,
    /// `Some(Unmatched)` = this token is an opening delimiter whose closing has not been found yet.
    /// `Some(Matched(idx))` = this token is an opening delimiter with its closing at index `idx`.
    /// `None` = not an opening token.
    pub opening: Option<OpeningState>,
    /// `Some(_)` = this token is a closing delimiter.
    pub closing: Option<ClosingAnnotation>,
    /// `Some(name)` = this token is the first word of a command (e.g. `git` in `git commit`).
    pub command_word: Option<String>,
}

impl Annotations {
    /// Returns `true` if no annotations have been set on this token.
    pub fn is_empty(&self) -> bool {
        *self == Annotations::default()
    }

    pub fn with_is_inside_single_quotes(mut self) -> Self {
        self.is_inside_single_quotes = true;
        self
    }

    pub fn with_is_inside_double_quotes(mut self) -> Self {
        self.is_inside_double_quotes = true;
        self
    }
}

#[derive(Debug, Clone)]
pub struct AnnotatedToken {
    pub token: Token,
    pub annotations: Annotations,
}

impl AnnotatedToken {
    pub fn new(token: Token) -> Self {
        Self {
            token,
            annotations: Annotations::default(),
        }
    }
}

#[derive(Debug)]
pub struct DParser {
    tokens: Vec<AnnotatedToken>,

    current_command_range: Option<RangeInclusive<usize>>,
}

impl DParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens: tokens.into_iter().map(AnnotatedToken::new).collect(),

            current_command_range: None,
        }
    }

    pub fn from(input: &str) -> Self {
        let tokens = collect_tokens_include_whitespace(input);
        Self::new(tokens)
    }

    #[allow(dead_code)]
    pub fn tokens(&self) -> &[AnnotatedToken] {
        &self.tokens
    }

    pub fn into_tokens(self) -> Vec<AnnotatedToken> {
        self.tokens
    }

    fn nested_opening_satisfied(
        token: &Token,
        current_nesting: Option<&TokenKind>,
        is_command_extraction: bool,
    ) -> bool {
        match token.kind {
            TokenKind::Quote | TokenKind::SingleQuote if is_command_extraction => false,
            TokenKind::Backtick | TokenKind::Quote | TokenKind::SingleQuote => {
                if Some(&token.kind) == current_nesting {
                    // backtick or quote is acting as closer
                    false
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    fn nested_closing_satisfied(token: &Token, current_nesting: Option<&TokenKind>) -> bool {
        let current_nesting = match current_nesting {
            Some(v) => v,
            None => return false,
        };
        match (&token.kind, current_nesting) {
            (TokenKind::RParen, TokenKind::LParen) => true,
            (TokenKind::RParen, TokenKind::CmdSubst) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstIn) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstOut) => true,
            (TokenKind::RParen, TokenKind::ExtGlob(_)) => true,
            (TokenKind::RBrace, TokenKind::ParamExpansion) => true,
            (TokenKind::RBrace, TokenKind::LBrace) => true,
            (TokenKind::DoubleRParen, TokenKind::ArithSubst) => true,
            (TokenKind::Backtick, TokenKind::Backtick) => true,
            (TokenKind::DoubleRBracket, TokenKind::DoubleLBracket) => true,
            (TokenKind::Quote, TokenKind::Quote) => true,
            (TokenKind::SingleQuote, TokenKind::SingleQuote) => true,
            (TokenKind::Esac, TokenKind::Case) => true,
            (TokenKind::Done, TokenKind::For) => true,
            (TokenKind::Done, TokenKind::While) => true,
            (TokenKind::Done, TokenKind::Until) => true,
            (TokenKind::Fi, TokenKind::If) => true,
            _ => false,
        }
    }

    pub fn walk_to_end(&mut self) {
        self.walk(None);
    }

    pub fn walk_to_cursor(&mut self, cursor_byte_pos: usize) {
        self.walk(Some(cursor_byte_pos));
    }

    fn walk(&mut self, cursor_byte_pos: Option<usize>) {
        // Walk through the tokens until we reach the end or the cursor position, updating nestings and heredocs along the way

        // echo $(( grep 1 + 2      # command is grep
        // echo $(( grep 1 + 2 )    # command is grep
        // echo $(( grep 1 + 2 ))   # command is echo, since the cursor is after the closing ))

        // The index of the last opening nesting token and its kind
        let mut nestings: Vec<(usize, TokenKind)> = Vec::new();
        // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings
        let mut heredocs: VecDeque<(usize, String, bool)> = VecDeque::new();

        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        let mut previous_token: Option<AnnotatedToken> = None;

        let mut idx = 0;
        while idx < self.tokens.len() {
            // When closing an ArithSubst, two consecutive ) tokens are required.
            // Merge them into a single DoubleRParen by modifying self.tokens[idx] in place
            // and removing the second ) from the vector.
            if nestings.last().map(|(_, k)| k) == Some(&TokenKind::ArithSubst)
                && self.tokens[idx].token.kind == TokenKind::RParen
                && idx + 1 < self.tokens.len()
                && self.tokens[idx + 1].token.kind == TokenKind::RParen
            {
                let second = self.tokens.remove(idx + 1);
                self.tokens[idx].token.value.push_str(&second.token.value);
                self.tokens[idx].token.kind = TokenKind::DoubleRParen;
            }

            // Something like `echo foo=bar` is not an assignment.
            if self.current_command_range.is_some()
                && self.tokens[idx].token.kind.is_word()
                && idx + 1 < self.tokens.len()
                && self.tokens[idx + 1].token.kind == TokenKind::Assignment
            {
                let second = self.tokens.remove(idx + 1);
                self.tokens[idx].token.value.push_str(&second.token.value);
                if idx + 1 < self.tokens.len() && self.tokens[idx + 1].token.kind.is_word() {
                    let third = self.tokens.remove(idx + 1);
                    self.tokens[idx].token.value.push_str(&third.token.value);
                }
            }

            // Clone the token so we can match on it while still mutating self.tokens[idx].annotation.
            let token = self.tokens[idx].token.clone();

            let word_is_part_of_assignment = if token.kind.is_word() {
                previous_token
                    .as_ref()
                    .is_some_and(|token| matches!(token.token.kind, TokenKind::Assignment))
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos.is_some_and(|pos| {
                self.tokens[idx]
                    .token
                    .byte_range()
                    .to_inclusive()
                    .contains(&pos)
            });
            let token_strictly_contains_cursor = cursor_byte_pos
                .is_some_and(|pos| self.tokens[idx].token.byte_range().contains(&pos));
            let cursor_at_start_of_token =
                cursor_byte_pos.is_some_and(|pos| pos == self.tokens[idx].token.byte_range().start);

            let cursor_part_way_through_token =
                token_inclusively_contains_cursor && !cursor_at_start_of_token;

            if token_strictly_contains_cursor {
                stop_parsing_at_command_boundary = true;
            }

            if cfg!(test) {
                dbg!(
                    "Token: {:?}, Nestings: {:?}, Heredocs: {:?}, Current command range: {:?}",
                    &token,
                    &nestings,
                    &heredocs,
                    &self.current_command_range
                );
            }

            match &token.kind {
                TokenKind::LBrace
                | TokenKind::Quote
                | TokenKind::SingleQuote
                | TokenKind::DoubleLBracket
                | TokenKind::Backtick
                | TokenKind::CmdSubst
                | TokenKind::ArithSubst
                | TokenKind::ArithCommand
                | TokenKind::ParamExpansion
                | TokenKind::ProcessSubstIn
                | TokenKind::ProcessSubstOut
                | TokenKind::ExtGlob(_)
                | TokenKind::If
                | TokenKind::Case
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until
                    if Self::nested_opening_satisfied(
                        &token,
                        nestings.last().map(|(_, k)| k),
                        cursor_byte_pos.is_some(),
                    ) =>
                {
                    self.tokens[idx].annotations.opening = Some(OpeningState::Unmatched);

                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    }
                    nestings.push((idx, token.kind.clone()));
                    command_start_stack.push(self.current_command_range.clone());
                    self.current_command_range = None; // set for next word after this
                }
                TokenKind::HereDoc { delimiter, quoted }
                | TokenKind::HereDocDash { delimiter, quoted } => {
                    self.tokens[idx].annotations.opening = Some(OpeningState::Unmatched);

                    heredocs.push_back((idx, delimiter.clone(), *quoted));
                }
                TokenKind::RParen
                | TokenKind::DoubleRParen
                | TokenKind::Quote
                | TokenKind::SingleQuote
                | TokenKind::RBrace
                | TokenKind::Backtick
                | TokenKind::DoubleRBracket
                | TokenKind::Esac
                | TokenKind::Done
                | TokenKind::Fi
                    if Self::nested_closing_satisfied(&token, nestings.last().map(|(_, k)| k)) =>
                {
                    let (opening_idx, _kind) = nestings.pop().unwrap();
                    self.tokens[idx].annotations.closing = Some(ClosingAnnotation {
                        opening_idx,
                        is_auto_inserted: false,
                    });

                    let current_command_range_contains_cursor =
                        cursor_byte_pos.is_some_and(|pos| {
                            self.current_command_range.as_ref().is_some_and(|r| {
                                r.clone().any(|idx| {
                                    self.tokens[idx]
                                        .token
                                        .byte_range()
                                        .to_inclusive()
                                        .contains(&pos)
                                })
                            })
                        });

                    if stop_parsing_at_command_boundary
                        && !cursor_part_way_through_token
                        && current_command_range_contains_cursor
                    {
                        // cursor_part_way_through_token is used to handle multi closing character tokens like )) and ]]
                        // echo $((10 * 2█))      -> cursor context is: 10 * 2
                        // echo $((10 * 2)█)      -> cursor context is: echo $((10 * 2))
                        // dbg!("Stopping parsing at command boundary");
                        break;
                    }

                    if let Some(prev_command_range) = command_start_stack.pop() {
                        self.current_command_range = prev_command_range;
                        if let Some(range) = &mut self.current_command_range {
                            *range = *range.start()..=idx;
                        }
                    }
                }
                TokenKind::Assignment => {
                    // When an assignment operator immediately follows a word (e.g. `FOO=1`),
                    // retroactively annotate that word as an environment variable name and
                    // remove the spurious command_word annotation it received earlier.
                    //
                    // Only do this when there is no active command yet, or when the only
                    // token in the active command range is the immediately preceding word
                    // (i.e. that word started the command range and now needs to be
                    // reinterpreted as an env-var assignment instead).  Otherwise the `=`
                    // is part of an argument to an existing command (e.g. the `go=` in
                    // `chmod go=,go-st /some/path`) and must not be turned into an
                    // env-var assignment.
                    let prev_is_lone_command_start = match &self.current_command_range {
                        Some(range) => *range.start() == idx - 1 && *range.end() == idx - 1,
                        None => true,
                    };
                    if prev_is_lone_command_start
                        && previous_token
                            .as_ref()
                            .is_some_and(|t| t.token.kind.is_word())
                    {
                        self.tokens[idx - 1].annotations.is_env_var = true;
                        self.tokens[idx - 1].annotations.command_word = None;
                    }
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }
                TokenKind::Word(_) if word_is_part_of_assignment => {
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }

                    if stop_parsing_at_command_boundary || token_inclusively_contains_cursor {
                        break;
                    }
                    self.current_command_range = None;
                }
                TokenKind::Word(word)
                    if heredocs
                        .front()
                        .is_some_and(|(heredoc_opening_idx, delim, _quoted)| {
                            let word_matches = delim == word;
                            let in_a_more_recent_nesting = nestings
                                .last()
                                .is_some_and(|(idx, _)| *idx > *heredoc_opening_idx);

                            word_matches && !in_a_more_recent_nesting
                        }) =>
                {
                    let (opening_idx, _, _) = heredocs.pop_front().unwrap();
                    self.tokens[idx].annotations.closing = Some(ClosingAnnotation {
                        opening_idx,
                        is_auto_inserted: false,
                    });
                }

                // These keywords and operators introduce a new command; reset the command
                // context so the first word after them receives the command_word annotation.
                TokenKind::And
                | TokenKind::Or
                | TokenKind::Pipe
                | TokenKind::Semicolon
                | TokenKind::Background
                | TokenKind::DoubleSemicolon
                | TokenKind::Do
                | TokenKind::Then
                | TokenKind::Elif
                | TokenKind::Else => {
                    if stop_parsing_at_command_boundary {
                        break;
                    }
                    self.current_command_range = None;
                }
                TokenKind::Whitespace(_) => {
                    if token_inclusively_contains_cursor
                        && let Some(range) = &mut self.current_command_range
                    {
                        *range = *range.start()..=idx;
                    }

                    if token_strictly_contains_cursor
                        && stop_parsing_at_command_boundary
                        && self.current_command_range.is_none()
                    {
                        // Stop parsing
                        self.current_command_range = Some(idx..=idx);
                        break;
                    }
                }

                _ => {
                    let in_single_quote = {
                        let last_nesting_is_single_quote_idx = nestings
                            .last()
                            .filter(|(_, k)| *k == TokenKind::SingleQuote)
                            .map(|(idx, _)| *idx);
                        let cur_heredoc_is_quoted_idx = heredocs
                            .front()
                            .filter(|(_, _, quoted)| *quoted)
                            .map(|(idx, _, _)| *idx);
                        match (last_nesting_is_single_quote_idx, cur_heredoc_is_quoted_idx) {
                            (Some(nesting_idx), Some(heredoc_idx)) => nesting_idx > heredoc_idx,
                            (Some(_), None) => true,
                            (None, Some(_)) => true,
                            (None, None) => false,
                        }
                    };
                    let in_double_quote = {
                        let last_nesting_is_double_quote_idx = nestings
                            .last()
                            .filter(|(_, k)| *k == TokenKind::Quote)
                            .map(|(idx, _)| *idx);
                        let cur_heredoc_is_unquoted_idx = heredocs
                            .front()
                            .filter(|(_, _, quoted)| !*quoted)
                            .map(|(idx, _, _)| *idx);
                        match (
                            last_nesting_is_double_quote_idx,
                            cur_heredoc_is_unquoted_idx,
                        ) {
                            (Some(nesting_idx), Some(heredoc_idx)) => nesting_idx > heredoc_idx,
                            (Some(_), None) => true,
                            (None, Some(_)) => true,
                            (None, None) => false,
                        }
                    };

                    if in_single_quote {
                        self.tokens[idx].annotations.is_inside_single_quotes = true;
                    } else if in_double_quote {
                        self.tokens[idx].annotations.is_inside_double_quotes = true;
                    }

                    if token.kind == TokenKind::Comment {
                        self.tokens[idx].annotations.is_comment = true;
                    }

                    if token.kind.is_word() && !in_single_quote {
                        if let Some(prev_token) = &previous_token {
                            if prev_token.token.kind == TokenKind::Dollar {
                                self.tokens[idx].annotations.is_env_var = true;
                                self.tokens[idx.saturating_sub(1)].annotations.is_env_var = true;
                            } else if !in_double_quote && self.current_command_range.is_none() {
                                self.tokens[idx].annotations.command_word =
                                    Some(self.tokens[idx].token.value.clone());
                            }

                            // Extend the command word into this one
                            if let Some(start_of_command) =
                                prev_token.annotations.command_word.as_ref()
                            {
                                let full_command =
                                    start_of_command.clone() + &self.tokens[idx].token.value;
                                self.tokens[idx].annotations.command_word =
                                    Some(full_command.clone());

                                for prev_command_token in self.tokens[..idx].iter_mut().rev() {
                                    // println!("Checking if we should extend command word annotation to token '{:?}' with value '{}'", prev_command_token.token.kind, prev_command_token.token.value);
                                    if prev_command_token.annotations.command_word.as_ref()
                                        == Some(start_of_command)
                                    {
                                        // println!("Extending command word annotation from '{}' to '{}'", start_of_command, full_command);
                                        prev_command_token.annotations.command_word =
                                            Some(full_command.clone());
                                    } else {
                                        break;
                                    }
                                }
                            }
                        } else if !in_double_quote {
                            self.tokens[idx].annotations.command_word =
                                Some(self.tokens[idx].token.value.clone());
                        }
                    }

                    // A Comment token must never start a command range or be
                    // tagged as a command word.
                    if self.current_command_range.is_none()
                        && !in_double_quote
                        && !in_single_quote
                        && token.kind != TokenKind::Comment
                    {
                        self.tokens[idx].annotations.command_word =
                            Some(self.tokens[idx].token.value.clone());

                        self.current_command_range = Some(idx..=idx);
                    } else if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }
            }

            previous_token = Some(self.tokens[idx].clone());
            idx += 1;
        }

        if cfg!(test) {
            dbg!("Final nestings:");
            dbg!(&nestings);
        }

        // Mark the opening tokens with the closing tokens:
        // We need to collect the updates first to avoid mutable borrow issues
        let mut updates = Vec::new();
        for (idx, annotated_token) in self.tokens.iter().enumerate() {
            if let Some(closing) = &annotated_token.annotations.closing {
                updates.push((closing.opening_idx, idx));
            }
        }

        for (opening_idx, closing_idx) in updates {
            self.tokens[opening_idx].annotations.opening = Some(OpeningState::Matched(closing_idx));
        }
    }

    pub fn needs_more_input(&self) -> bool {
        self.tokens
            .iter()
            .any(|t| t.annotations.opening == Some(OpeningState::Unmatched))
    }

    pub fn get_current_command_tokens(&self) -> &[AnnotatedToken] {
        match &self.current_command_range {
            Some(range) => &self.tokens[range.clone()],
            None => &[],
        }
    }

    #[allow(dead_code)]
    pub fn get_current_command_str(&self) -> String {
        self.get_current_command_tokens()
            .iter()
            .map(|t| t.token.value.to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns the closing character that should be automatically inserted after the character `c`
    /// was typed at byte position `just_inserted_pos`.
    ///
    /// `self` is the **stale** (pre-insertion) formatted buffer — i.e. the state of the buffer
    /// *before* `c` was typed.  This is `self.formatted_buffer_cache` in `App`.
    ///
    /// - `{`, `[`, `(` are unambiguously openers and always produce a closing counterpart.
    /// - `"`, `'`, `` ` `` are ambiguous: they close when there is already an unmatched opener of
    ///   the same kind before `just_inserted_pos` in the stale buffer; otherwise they open.
    /// - Returns `None` when `just_inserted_pos` falls inside an already-matched single- or
    ///   double-quoted string, or when the character at `just_inserted_pos` in the stale buffer
    ///   is the start of (or inside) a word token.
    pub fn closing_char_to_insert(
        tokens: &[AnnotatedToken],
        c: char,
        just_inserted_pos: usize,
    ) -> Option<char> {
        // Never auto-close inside a comment.
        if tokens.iter().any(|t| {
            t.token
                .byte_range()
                .to_inclusive()
                .contains(&just_inserted_pos)
                && matches!(t.token.kind, TokenKind::Comment)
        }) {
            return None;
        }

        // Compute context flags once and reuse throughout.
        let is_inside_single_quote = tokens.iter().any(|t| {
            if let Some(OpeningState::Matched(close_idx)) = t.annotations.opening {
                if t.token.kind == TokenKind::SingleQuote {
                    let open_end = t.token.byte_range().end;
                    let close_start = tokens[close_idx].token.byte_range().start;
                    return open_end <= just_inserted_pos && just_inserted_pos <= close_start;
                }
            }
            false
        });

        let is_inside_double_quote = tokens.iter().any(|t| {
            if let Some(OpeningState::Matched(close_idx)) = t.annotations.opening {
                if t.token.kind == TokenKind::Quote {
                    let open_end = t.token.byte_range().end;
                    let close_start = tokens[close_idx].token.byte_range().start;
                    return open_end <= just_inserted_pos && just_inserted_pos <= close_start;
                }
            }
            false
        });

        // If a word token starts at or contains `just_inserted_pos`, we are inserting the quote
        // immediately before (or inside) an existing word. Auto-closing would wrap only an empty
        // string, leaving the word outside the quotes. E.g., `"` before `bar` in `foo bar` should
        // yield `foo "bar`, not `foo "bar"`.
        let is_before_word = tokens
            .iter()
            .any(|t| t.token.kind.is_word() && t.token.byte_range().contains(&just_inserted_pos));

        // Inside a matched quoted string the typed character is literal content – don't
        // auto-close. Exception: `$` expansions are active inside double quotes, so `$(` → `)`
        // and `${` → `}` are still auto-closed there (but not inside single quotes).
        if is_inside_single_quote || is_inside_double_quote {
            if is_inside_double_quote && !is_before_word && matches!(c, '(' | '{') {
                let prev_token_kind = tokens
                    .iter()
                    .rev()
                    .find(|t| t.token.byte_range().end == just_inserted_pos)
                    .map(|t| &t.token.kind);
                let closing = match (c, prev_token_kind) {
                    ('(', Some(TokenKind::Dollar | TokenKind::CmdSubst)) => Some(')'),
                    ('{', Some(TokenKind::Dollar)) => Some('}'),
                    _ => None,
                };
                if closing.is_some() {
                    return closing;
                }
            }
            return None;
        }

        if is_before_word {
            return None;
        }

        // Unambiguously opening characters – always auto-close.
        match c {
            '{' => return Some('}'),
            '[' => return Some(']'),
            '(' => return Some(')'),
            _ => {}
        }

        // Ambiguous characters: consult the stale token annotations.
        let (closing, opener_kind) = match c {
            '"' => ('"', TokenKind::Quote),
            '\'' => ('\'', TokenKind::SingleQuote),
            '`' => ('`', TokenKind::Backtick),
            _ => return None,
        };

        // If there is already an unmatched opener of the same kind strictly before the
        // insertion point, the character just typed is closing it – don't auto-insert.
        let has_unmatched_opener = tokens.iter().any(|p| {
            p.token.byte_range().start < just_inserted_pos
                && p.token.kind == opener_kind
                && p.annotations.opening == Some(OpeningState::Unmatched)
        });

        if has_unmatched_opener {
            None
        } else {
            Some(closing)
        }
    }

    /// Returns `buffer` with any trailing auto-inserted closing tokens stripped.
    /// TODO: think of good ux for when the user wants to search history with auto inserted chars.
    #[allow(dead_code)]
    pub fn buffer_without_auto_inserted_suffix<'buf>(
        tokens: &[AnnotatedToken],
        buffer: &'buf str,
    ) -> &'buf str {
        let trailing_len: usize = tokens
            .iter()
            .rev()
            .take_while(|t| {
                t.annotations
                    .closing
                    .as_ref()
                    .is_some_and(|c| c.is_auto_inserted)
            })
            .map(|t| t.token.value.len())
            .sum();
        &buffer[..buffer.len().saturating_sub(trailing_len)]
    }

    pub fn transfer_auto_inserted_flags(
        old_tokens: &[AnnotatedToken],
        new_tokens: &mut [AnnotatedToken],
    ) {
        // Go from the left while we see identical tokens and mark any closing tokens in new_tokens as auto-inserted if the corresponding token in old_tokens was auto-inserted.
        for (old, new) in old_tokens.iter().zip(new_tokens.iter_mut()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let Some(ClosingAnnotation {
                opening_idx: old_opening_idx,
                is_auto_inserted: true,
            }) = &old.annotations.closing
                && let Some(new_closing) = &mut new.annotations.closing
                && *old_opening_idx == new_closing.opening_idx
            {
                new_closing.is_auto_inserted = true;
            }
        }

        // Go from the right while we see identical tokens and do the same.
        for (old, new) in old_tokens.iter().rev().zip(new_tokens.iter_mut().rev()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let Some(ClosingAnnotation {
                is_auto_inserted: true,
                ..
            }) = &old.annotations.closing
                && let Some(new_closing) = &mut new.annotations.closing
            {
                new_closing.is_auto_inserted = true;
            }
        }
    }
}

// Implicitly tested by command acceptance and tab_completion_context
// Just a few tests here
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nested_commands() {
        let input = r#"     echo $(ls $(echo nested) | grep pattern) > output.txt       "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, input.trim_start());
    }

    #[test]
    fn test_in_nested_command() {
        let input = r#"echo $(ls $(   echo nest    "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, "echo nest    ");
    }

    #[test]
    fn test_pipeline() {
        let input = r#"echo "héllo" && echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, r#"echo "wörld""#);
    }

    #[test]
    fn test_pipeline_with_nesting_1() {
        let input = r#"echo "héllo" && echo $(( bar "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar "#);
    }

    #[test]
    fn test_pipeline_with_nesting_2() {
        let input = r#"echo "héllo" && echo $(( bar ) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar ) "#);
    }

    #[test]
    fn test_pipeline_with_nesting_3() {
        let input = r#"echo "héllo" && echo $(( bar )) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo $(( bar )) "#);
    }

    #[test]
    fn test_annotations() {
        let input = r#"echo héllo && echo 'wörld'"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "héllo");
        assert_eq!(tokens[2].annotations, Annotations::default());
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "&&");
        assert_eq!(tokens[4].annotations, Annotations::default());
        assert_eq!(tokens[5].token.value, " ");
        assert_eq!(tokens[6].token.value, "echo");
        assert_eq!(tokens[6].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[7].token.value, " ");
        assert_eq!(tokens[8].token.value, "'");
        assert_eq!(
            tokens[8].annotations.opening,
            Some(OpeningState::Matched(10))
        );
        assert_eq!(tokens[9].token.value, "wörld");
        assert!(tokens[9].annotations.is_inside_single_quotes);
        assert_eq!(tokens[10].token.value, "'");
        assert_eq!(
            tokens[10].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 8,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_double_quote_annotations() {
        let input = r#"echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "\"");
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(4))
        );
        assert_eq!(tokens[3].token.value, "wörld");
        assert!(tokens[3].annotations.is_inside_double_quotes);
        assert_eq!(tokens[4].token.value, "\"");
        assert_eq!(
            tokens[4].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_heredoc_annotations() {
        let input = "cat <<A <<-B\nline1\nA\nline2\nB\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }
        assert_eq!(tokens[0].token.value, "cat");
        assert_eq!(tokens[0].annotations.command_word, Some("cat".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "<<A");
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(8))
        );
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "<<-B");
        assert_eq!(
            tokens[4].annotations.opening,
            Some(OpeningState::Matched(12))
        );
        assert_eq!(tokens[5].token.value, "\n");
        assert_eq!(
            tokens[5].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[6].token.value, "line1");
        assert_eq!(
            tokens[6].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[7].token.value, "\n");
        assert_eq!(
            tokens[7].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[8].token.value, "A");
        assert_eq!(
            tokens[8].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false
            })
        );
        assert_eq!(tokens[9].token.value, "\n");
        assert_eq!(
            tokens[9].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[10].token.value, "line2");
        assert_eq!(
            tokens[10].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[11].token.value, "\n");
        assert_eq!(
            tokens[11].annotations,
            Annotations::default().with_is_inside_double_quotes()
        );
        assert_eq!(tokens[12].token.value, "B");
        assert_eq!(
            tokens[12].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 4,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_pipe_and_separator() {
        let input = r#"echo "héllo" |& cat"#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), "cat");
    }

    #[test]
    fn test_pipe_and_separator_with_nesting() {
        let input = r#"echo "héllo" |& echo $(( bar "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar "#);
    }

    #[test]
    fn test_background_separator() {
        let input = r#"echo "héllo" & echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo "wörld""#);
    }

    #[test]
    fn test_double_semicolon_separator() {
        let input = r#"echo "héllo";; echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo "wörld""#);
    }

    #[test]
    fn test_multiline_string_annotations() {
        let input = "echo 'line1\nline2'";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "'");
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(6))
        );
        assert_eq!(tokens[3].token.value, "line1");
        assert!(tokens[3].annotations.is_inside_single_quotes);
        assert_eq!(tokens[4].token.kind, TokenKind::Newline);
        assert!(tokens[4].annotations.is_inside_single_quotes);
        assert_eq!(tokens[5].token.value, "line2");
        assert!(tokens[5].annotations.is_inside_single_quotes);
        assert_eq!(tokens[6].token.value, "'");
        assert_eq!(
            tokens[6].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_arith_subst_annotations() {
        // The two consecutive ) tokens that close an ArithSubst are merged into a single
        // DoubleRParen token with value "))" covering both characters.  The phantom second )
        // is removed from the token list entirely, so subsequent tokens have the correct index
        // as if the second ) never existed.
        let input = r#"echo $(( bar ))"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // After merging: echo (0), ' ' (1), $(( (2), ' ' (3), bar (4), ' ' (5), )) (6)
        // The phantom second ) is gone; total token count is 7.
        assert_eq!(tokens.len(), 7);

        assert_eq!(tokens[2].token.kind, TokenKind::ArithSubst);
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(6))
        );

        assert_eq!(tokens[6].token.kind, TokenKind::DoubleRParen);
        assert_eq!(tokens[6].token.value, "))");
        assert_eq!(
            tokens[6].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_env_var_annotations() {
        let input = r#"echo $HOME"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "$");
        assert!(tokens[2].annotations.is_env_var);
        assert_eq!(tokens[3].token.value, "HOME");
        assert!(tokens[3].annotations.is_env_var);
    }

    #[test]
    fn test_env_var_in_double_quotes_annotations() {
        let input = r#"echo "prefix$HOME""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }
        // tokens: echo(0) ' '(1) "(2) prefix(3) $(4) HOME(5) "(6)
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[2].token.value, "\"");
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(6))
        );

        assert_eq!(tokens[3].token.value, "prefix");
        assert!(tokens[3].annotations.is_inside_double_quotes,);
        assert!(!tokens[3].annotations.is_env_var,);

        assert_eq!(tokens[4].token.value, "$");
        assert!(tokens[4].annotations.is_inside_double_quotes);
        assert!(tokens[4].annotations.is_env_var);

        assert_eq!(tokens[5].token.value, "HOME");
        assert!(tokens[5].annotations.is_inside_double_quotes);
        assert!(tokens[5].annotations.is_env_var);

        assert_eq!(tokens[6].token.value, "\"");
        assert_eq!(
            tokens[6].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false
            })
        );
    }

    #[test]
    fn test_first_word_of_quotes() {
        let input = r#"echo "fi""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "\"");
        assert_eq!(tokens[3].token.value, "fi");
        assert!(tokens[3].annotations.is_inside_double_quotes);
        assert!(tokens[3].annotations.command_word.is_none());
    }

    // ── closing_char_to_insert ───────────────────────────────────────────────
    // These tests pass a *stale* (pre-insertion) FormattedBuffer to
    // closing_char_to_insert, mirroring how App uses formatted_buffer_cache.

    #[test]
    fn closing_char_for_opening_double_quote() {
        // Stale buffer is "echo " (before the " was typed).
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            Some('"')
        );
    }

    #[test]
    fn no_closing_char_for_closing_double_quote() {
        // Stale buffer is `echo "hello` (before the closing " was typed).
        let stale = r#"echo "hello"#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_for_opening_single_quote() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            Some('\'')
        );
    }

    #[test]
    fn no_closing_char_for_closing_single_quote() {
        let stale = "echo 'hello";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_for_opening_brace() {
        // { is never ambiguous; always produces a closing }.
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '{', just_inserted_pos),
            Some('}')
        );
    }

    #[test]
    fn closing_char_for_opening_backtick() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '`', just_inserted_pos),
            Some('`')
        );
    }

    #[test]
    fn no_closing_char_for_closing_backtick() {
        // Stale buffer is `echo `ls` (before the closing backtick was typed).
        let stale = "echo `ls";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '`', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_for_unrecognised_character() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), 'a', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_second_quote_pair_after_first_closed() {
        // `echo "a" ` – the first pair is closed; the next " opens a new pair.
        let stale = r#"echo "a" "#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            Some('"')
        );
    }

    #[test]
    fn closing_char_dont_insert_in_comment() {
        // `echo # comment ` – the # starts a comment, so the next " is just a literal character, not an opener.
        let stale = "echo # comment ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    // ── Case 1: insertion inside an already-matched quoted string ────────────

    #[test]
    fn no_closing_char_single_quote_inserted_inside_double_quoted_string() {
        // Buffer is `"abcde"` (fully matched). Cursor between `b` and `c` (pos 3).
        // Inserting `'` inside an existing double-quoted string should not auto-close.
        let stale = r#""abcde""#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        // Position 3 is inside the word `abcde` (byte range 1..6), which is inside the quotes.
        let just_inserted_pos = 3;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_quote_inserted_at_boundary_before_closing_double_quote() {
        // Buffer is `"abcde"`. Cursor at position 6, right before the closing `"`.
        // Inserting `'` there should not auto-close.
        let stale = r#""abcde""#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        // `"` is at 0, `abcde` is at 1..6, closing `"` is at 6.
        let just_inserted_pos = 6;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_double_quote_inserted_inside_single_quoted_string() {
        // Buffer is `'hello world'` (fully matched single-quoted string).
        // Cursor at position 5 (inside the content).
        // Inserting `"` inside a single-quoted string should not auto-close.
        let stale = "'hello world'";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 5;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_single_quote_inserted_inside_single_quoted_string() {
        // Buffer is `'hello world'` (fully matched). Cursor at position 5.
        // Inserting another `'` inside should not auto-close.
        let stale = "'hello world'";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 5;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    // ── Case 2: insertion immediately before a word token ────────────────────

    #[test]
    fn no_closing_char_double_quote_inserted_before_word() {
        // Buffer is `foo bar`. Cursor at position 4 (start of `bar`).
        // Inserting `"` before a word should give `foo "bar`, not `foo "bar"`.
        let stale = "foo bar";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 4; // byte offset of `b` in `bar`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_single_quote_inserted_before_word() {
        // Buffer is `foo bar`. Cursor at position 4 (start of `bar`).
        // Inserting `'` before a word should not auto-close.
        let stale = "foo bar";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 4;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_double_quote_inserted_within_word() {
        // Buffer is `foobar`. Cursor at position 3 (inside the word).
        // Inserting `"` inside a word should not auto-close.
        let stale = "foobar";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 3;
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_double_quote_after_word_is_inserted() {
        // Buffer is `foo`. Cursor at the end (position 3, after the word).
        // Inserting `"` after a word (not before one) should still auto-close.
        let stale = "foo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len(); // position 4, past whitespace
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            Some('"')
        );
    }

    // ── Dollar-prefix auto-close inside double-quoted strings ────────────────

    #[test]
    fn paren_auto_closed_after_dollar_inside_double_quoted_string() {
        // Stale buffer: `"$"` – the `"` pair is matched (auto-inserted closing).
        // Cursor is at position 2 (after `$`, before the closing `"`).
        // Typing `(` should still produce `)` because `$(` is a valid expansion.
        let stale = r#""$""#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 2; // after `$`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '(', just_inserted_pos),
            Some(')')
        );
    }

    #[test]
    fn paren_auto_closed_after_cmdsubst_inside_double_quoted_string() {
        // Stale buffer: `"$()"` – `$(` is a CmdSubst token, `)` auto-inserted, `"` matched.
        // Cursor is at position 3 (after `$(`, before the auto-inserted `)`).
        // Typing `(` should produce `)` to allow `$((1+2))`.
        let stale = r#""$()""#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 3; // after `$(`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '(', just_inserted_pos),
            Some(')')
        );
    }

    #[test]
    fn brace_auto_closed_after_dollar_inside_double_quoted_string() {
        // Stale buffer: `"$"` – matched double-quoted pair.
        // Cursor at position 2 (after `$`).
        // Typing `{` should produce `}` because `${var}` is a valid expansion.
        let stale = r#""$""#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 2; // after `$`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '{', just_inserted_pos),
            Some('}')
        );
    }

    #[test]
    fn no_paren_auto_close_after_dollar_inside_single_quoted_string() {
        // Stale buffer: `'$'` – single-quoted, `$` is literal; no expansion.
        // Cursor at position 2 (after `$`).
        // Typing `(` should NOT auto-close.
        let stale = "'$'";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 2; // after `$`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '(', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_brace_auto_close_after_dollar_inside_single_quoted_string() {
        // Stale buffer: `'$'` – single-quoted.
        // Typing `{` should NOT auto-close.
        let stale = "'$'";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = 2; // after `$`
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '{', just_inserted_pos),
            None
        );
    }

    #[test]
    fn test_heredoc_single_quoted_delimiter() {
        // Single-quoted delimiter: closing line is the bare word without quotes.
        let input = "cat <<'EOF'\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // <<'EOF' token should be an opening that is matched.
        assert_eq!(tokens[2].token.value, "<<'EOF'");
        assert!(tokens[2].annotations.opening.is_some());

        // Find the "EOF" closing token.
        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false,
            })
        );
    }

    #[test]
    fn test_heredoc_double_quoted_delimiter() {
        let input = "cat <<\"EOF\"\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // <<"EOF" token should be matched.
        assert_eq!(tokens[2].token.value, "<<\"EOF\"");
        assert!(tokens[2].annotations.opening.is_some());

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false,
            })
        );
    }

    #[test]
    fn test_heredoc_backslash_quoted_delimiter() {
        let input = "cat <<\\EOF\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // <<\EOF token should be matched.
        assert_eq!(tokens[2].token.value, "<<\\EOF");
        assert!(tokens[2].annotations.opening.is_some());

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false,
            })
        );
    }

    #[test]
    fn test_heredoc_mixed_quoted_delimiter() {
        // Partially-quoted delimiter: E'O'F is equivalent to EOF.
        let input = "cat <<E'O'F\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[2].token.value, "<<E'O'F");
        assert!(tokens[2].annotations.opening.is_some());

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 2,
                is_auto_inserted: false,
            })
        );
    }

    #[test]
    fn test_heredoc_before_open_quote() {
        // Partially-quoted delimiter: E'O'F is equivalent to EOF.
        let input = "cat <<E'O'F'\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(
            tokens[2].token.kind,
            TokenKind::HereDoc {
                delimiter: "EOF".to_string(),
                quoted: true
            }
        );
        assert_eq!(tokens[2].token.value, "<<E'O'F");
        assert!(tokens[2].annotations.opening == Some(OpeningState::Unmatched));

        assert_eq!(tokens[3].token.kind, TokenKind::SingleQuote);
        assert_eq!(tokens[4].token.kind, TokenKind::Newline);
        assert_eq!(tokens[5].token.kind, TokenKind::Word("hello".to_string()));
        assert_eq!(tokens[6].token.kind, TokenKind::Newline);
        // This is just a plain word, not a closing token for the heredoc because the stray ' after the delim opens a multiline single-quoted string that isn't closed until the end of the buffer. The heredoc is left unmatched.
        assert_eq!(tokens[7].token.kind, TokenKind::Word("EOF".to_string()));
    }

    #[test]
    fn test_comment_annotation() {
        let input = "echo hello # this is a comment";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "hello");
        assert_eq!(tokens[2].annotations, Annotations::default());
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "# this is a comment");
        assert!(tokens[4].annotations.is_comment);
    }

    #[test]
    fn env_var_in_double_quotes_has_env_var_color() {
        let input = r#"echo "$HOME/foo""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "\"");
        assert_eq!(tokens[3].token.value, "$");
        assert_eq!(tokens[3].annotations.is_env_var, true);
        assert_eq!(tokens[4].token.value, "HOME");
        assert_eq!(tokens[4].annotations.is_env_var, true);
        assert_eq!(tokens[5].token.value, "/foo");
        assert_eq!(tokens[5].annotations.is_env_var, false);
        assert_eq!(tokens[6].token.value, "\"");
    }

    #[test]
    fn test_env_var_starting_command() {
        let input = r#"$HOME/bin/echo"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "$");
        assert_eq!(tokens[0].annotations.is_env_var, true);
        assert_eq!(
            tokens[0].annotations.command_word.as_ref().unwrap(),
            "$HOME/bin/echo"
        );
        assert_eq!(tokens[1].token.value, "HOME");
        assert_eq!(tokens[1].annotations.is_env_var, true);
        assert_eq!(
            tokens[1].annotations.command_word.as_ref().unwrap(),
            "$HOME/bin/echo"
        );

        assert_eq!(tokens[2].token.value, "/bin/echo");
        assert_eq!(tokens[2].annotations.is_env_var, false);
        assert_eq!(
            tokens[2].annotations.command_word.as_ref().unwrap(),
            "$HOME/bin/echo"
        );
    }

    #[test]
    fn test_assignment_env_var_annotation() {
        // `FOO=1 echo hello`: FOO is the env-var name; echo is the command.
        let input = r#"FOO=1 echo hello"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // FOO – the variable name before `=`
        assert_eq!(tokens[0].token.value, "FOO");
        assert!(tokens[0].annotations.is_env_var);
        assert_eq!(tokens[0].annotations.command_word, None);

        // = – the assignment operator
        assert_eq!(tokens[1].token.value, "=");

        // 1 – the value on the right-hand side; not an env var
        assert_eq!(tokens[2].token.value, "1");
        assert!(!tokens[2].annotations.is_env_var);

        // echo – the command that follows the env-var prefix
        assert_eq!(tokens[4].token.value, "echo");
        assert_eq!(tokens[4].annotations.command_word, Some("echo".to_string()));

        // hello – a plain argument
        assert_eq!(tokens[6].token.value, "hello");
        assert_eq!(tokens[6].annotations, Annotations::default());
    }

    #[test]
    fn test_assignment_inside_command_args_not_env_var() {
        // `chmod go=,go-st /some/path`: the `go` to the left of `=` is an
        // argument to `chmod`, not an env-var assignment. It must therefore
        // not be tagged with is_env_var.
        let input = r#"chmod go=,go-st /some/path"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // chmod – the command word
        assert_eq!(tokens[0].token.value, "chmod");
        assert_eq!(
            tokens[0].annotations.command_word,
            Some("chmod".to_string())
        );

        // go – first argument fragment, NOT an env var
        assert_eq!(tokens[2].token.value, "go=,go-st");
        assert!(!tokens[2].annotations.is_env_var);
    }

    #[test]
    fn test_equal_sign_is_not_assignment() {
        // `chmod go=,go-st /some/path`: the `go` to the left of `=` is an
        // argument to `chmod`, not an env-var assignment. It must therefore
        // not be tagged with is_env_var.
        let input = r#"bar=baz chmod go=foo"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        assert_eq!(tokens[0].token.value, "bar");
        assert_eq!(tokens[1].token.kind, TokenKind::Assignment);
        assert_eq!(tokens[2].token.value, "baz");

        // chmod – the command word
        assert_eq!(tokens[4].token.value, "chmod");
        assert_eq!(
            tokens[4].annotations.command_word,
            Some("chmod".to_string())
        );

        assert_eq!(tokens[6].token.value, "go=foo");
    }

    #[test]
    fn test_comment_only_buffer_not_command() {
        // A buffer containing only a comment must not produce any token
        // annotated as a command word; the only token should be flagged as a
        // comment instead.
        let input = "# just a comment";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        for t in tokens {
            assert!(
                t.annotations.command_word.is_none(),
                "Comment-only buffer should not produce a command word, but token {:?} got command_word={:?}",
                t.token,
                t.annotations.command_word
            );
        }
        // At least one token must be flagged as a comment.
        assert!(tokens.iter().any(|t| t.annotations.is_comment));
    }

    #[test]
    fn test_for_loop_annotations() {
        // Verify that `for…done` is matched, `echo` inside the body gets the
        // command_word annotation, and `$i` is recognised as an env var.
        let input = r#"for i in {1..4}; do echo "Welcome $i";done"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?}", &t.token,);
            // dbg!("{:?}", &t.annotations);
        }

        // `for` – opening of the for…done block
        assert_eq!(tokens[0].token.kind, TokenKind::For);
        assert_eq!(tokens[0].token.value, "for");
        assert_eq!(
            tokens[0].annotations.opening,
            Some(OpeningState::Matched(21))
        );

        // `do` – keyword introducing the loop body; must NOT be the command_word
        assert_eq!(tokens[11].token.kind, TokenKind::Do);
        assert_eq!(tokens[11].token.value, "do");
        assert_eq!(tokens[11].annotations.command_word, None);

        // `echo` – first word of the command inside the loop body
        assert_eq!(tokens[13].token.value, "echo");
        assert_eq!(
            tokens[13].annotations.command_word,
            Some("echo".to_string())
        );

        // `"` – opening double-quote matched with its closing counterpart
        assert_eq!(tokens[15].token.value, "\"");
        assert_eq!(tokens[15].token.value, "\"");
        assert_eq!(
            tokens[15].annotations.opening,
            Some(OpeningState::Matched(19))
        );

        // `Welcome ` – inside double quotes
        assert_eq!(tokens[16].token.value, "Welcome ");
        assert!(tokens[16].annotations.is_inside_double_quotes);

        // `$` – env-var sigil inside double quotes
        assert_eq!(tokens[17].token.value, "$");
        assert!(tokens[17].annotations.is_env_var);
        assert!(tokens[17].annotations.is_inside_double_quotes);

        // `i` – env-var name inside double quotes
        assert_eq!(tokens[18].token.value, "i");
        assert!(tokens[18].annotations.is_env_var);
        assert!(tokens[18].annotations.is_inside_double_quotes);

        // closing `"` matched back to its opener
        assert_eq!(tokens[19].token.value, "\"");
        assert_eq!(
            tokens[19].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 15,
                is_auto_inserted: false
            })
        );

        // `done` – closing keyword matched back to `for`
        assert_eq!(tokens[21].token.value, "done");
        assert_eq!(
            tokens[21].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 0,
                is_auto_inserted: false
            })
        );
    }

    // ---- buffer_without_auto_inserted_suffix tests ----

    /// Helper: build a token list for `input` and mark the last token as auto-inserted closing.
    fn make_tokens_with_auto_inserted_suffix(input: &str) -> Vec<AnnotatedToken> {
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let mut tokens = parser.into_tokens();
        // Mark the final token as auto-inserted closing (simulate what the editor does).
        if let Some(last) = tokens.last_mut() {
            last.annotations.closing = Some(ClosingAnnotation {
                opening_idx: 0,
                is_auto_inserted: true,
            });
        }
        tokens
    }

    #[test]
    fn buffer_without_auto_inserted_suffix_no_auto_inserted() {
        // No auto-inserted tokens: buffer returned unchanged.
        let input = "echo hello";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.into_tokens();
        assert_eq!(
            DParser::buffer_without_auto_inserted_suffix(&tokens, input),
            input,
        );
    }

    #[test]
    fn buffer_without_auto_inserted_suffix_single_char_stripped() {
        // Buffer `echo "hello"` where the last `"` is auto-inserted.
        let input = r#"echo "hello""#;
        let tokens = make_tokens_with_auto_inserted_suffix(input);
        // The last token is `"` (one byte).
        assert_eq!(
            DParser::buffer_without_auto_inserted_suffix(&tokens, input),
            r#"echo "hello"#,
        );
    }

    #[test]
    fn buffer_without_auto_inserted_suffix_multiple_chars_stripped() {
        // Buffer `echo ({})` where both `}` and `)` are auto-inserted closing tokens.
        let input = "echo ({})";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let mut tokens = parser.into_tokens();
        // Verify there are at least 2 tokens and mark the last two as auto-inserted closing.
        let len = tokens.len();
        assert!(len >= 2);
        for tok in tokens[len - 2..].iter_mut() {
            tok.annotations.closing = Some(ClosingAnnotation {
                opening_idx: 0,
                is_auto_inserted: true,
            });
        }
        // Both `}` and `)` (1 char each) are stripped from "echo ({})".
        assert_eq!(
            DParser::buffer_without_auto_inserted_suffix(&tokens, input),
            "echo ({",
        );
    }

    #[test]
    fn buffer_without_auto_inserted_suffix_empty_tokens() {
        // Empty token slice: buffer returned unchanged.
        assert_eq!(
            DParser::buffer_without_auto_inserted_suffix(&[], "echo hello"),
            "echo hello",
        );
    }
}
