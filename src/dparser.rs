use flash::lexer::{Lexer, Token, TokenKind};
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};

use crate::app::auto_close::surround_closing_char;

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
    /// Nesting depth for opening and closing delimiter tokens, used for rainbow bracket
    /// colouring.  `0` is the outermost level.  `None` for non-delimiter tokens.
    pub bracket_depth: Option<usize>,
}

impl Annotations {
    /// Returns `true` if no annotations have been set on this token.
    #[allow(dead_code)]
    pub fn has_no_annotations(&self) -> bool {
        *self == Annotations::default()
    }

    #[allow(dead_code)]
    pub fn with_is_inside_single_quotes(mut self) -> Self {
        self.is_inside_single_quotes = true;
        self
    }

    #[allow(dead_code)]
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

    pub fn parse_and_annotate(input: &str) -> Vec<AnnotatedToken> {
        let _timer = crate::perf::PerfTimer::start("parse_and_annotate");
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        parser.into_tokens()
    }

    pub fn parse_and_transfer_auto_inserted_flags(
        input: &str,
        old_tokens: &[AnnotatedToken],
    ) -> Vec<AnnotatedToken> {
        let mut new_tokens = Self::parse_and_annotate(input);

        // Go from the left while we see identical tokens and mark any closing tokens in new_tokens as auto-inserted if the corresponding token in old_tokens was auto-inserted.
        for (old, new) in old_tokens.iter().zip(new_tokens.iter_mut()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let Some(ClosingAnnotation {
                opening_idx: old_opening_idx,
                is_auto_inserted: true,
            }) = &old.annotations.closing
            {
                match &mut new.annotations.closing {
                    Some(new_closing) if *old_opening_idx == new_closing.opening_idx => {
                        new_closing.is_auto_inserted = true;
                    }
                    None => {
                        // Token kind isn't paired by dparser nesting (e.g. `]`).
                        // Preserve the auto-inserted flag from the previous parse.
                        new.annotations.closing = Some(ClosingAnnotation {
                            opening_idx: *old_opening_idx,
                            is_auto_inserted: true,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Go from the right while we see identical tokens and do the same.
        for (old, new) in old_tokens.iter().rev().zip(new_tokens.iter_mut().rev()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let Some(ClosingAnnotation {
                opening_idx: old_opening_idx,
                is_auto_inserted: true,
            }) = &old.annotations.closing
            {
                match &mut new.annotations.closing {
                    Some(new_closing) => {
                        new_closing.is_auto_inserted = true;
                    }
                    None => {
                        new.annotations.closing = Some(ClosingAnnotation {
                            opening_idx: *old_opening_idx,
                            is_auto_inserted: true,
                        });
                    }
                }
            }
        }

        new_tokens
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
            TokenKind::LParen => {
                matches!(
                    current_nesting,
                    Some(TokenKind::ArithSubst | TokenKind::ArithCommand | TokenKind::LParen)
                )
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
            (TokenKind::DoubleRParen, TokenKind::ArithCommand) => true,
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

    fn is_builtin_like_reserved_word(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Return
                | TokenKind::Export
                | TokenKind::Complete
        )
    }

    fn is_in_header_context(
        tokens: &[AnnotatedToken],
        nestings: &[(usize, TokenKind)],
        idx: usize,
    ) -> bool {
        let Some((opening_idx, opening_kind)) = nestings.last() else {
            return false;
        };

        let since_opening = &tokens[opening_idx + 1..idx];
        match opening_kind {
            TokenKind::For => !since_opening
                .iter()
                .any(|t| matches!(t.token.kind, TokenKind::Do | TokenKind::In)),
            TokenKind::Case => !since_opening
                .iter()
                .any(|t| matches!(t.token.kind, TokenKind::In)),
            _ => false,
        }
    }

    fn active_heredoc_opening_idx(
        tokens: &[AnnotatedToken],
        heredocs: &VecDeque<(usize, String, bool, usize)>,
        nestings: &[(usize, TokenKind)],
        token: &Token,
    ) -> Option<usize> {
        let (heredoc_opening_idx, _, _, _) = heredocs.front()?;
        let heredoc_opening_idx = *heredoc_opening_idx;

        let opener = tokens.get(heredoc_opening_idx)?;
        let opener_line = opener.token.position.line;
        let body_started = token.position.line > opener_line
            || (matches!(token.kind, TokenKind::Newline)
                && token.byte_range().start >= opener.token.byte_range().end);
        if !body_started {
            return None;
        }

        let in_a_more_recent_nesting = nestings
            .last()
            .is_some_and(|(nesting_idx, _)| *nesting_idx > heredoc_opening_idx);
        if in_a_more_recent_nesting {
            return None;
        }

        Some(heredoc_opening_idx)
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
        // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings.
        // Each entry is (opening_token_idx, delimiter, is_quoted, depth_at_open).
        let mut heredocs: VecDeque<(usize, String, bool, usize)> = VecDeque::new();

        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        let mut previous_token: Option<AnnotatedToken> = None;

        // Set to true when a closing nesting restores a command range whose first token
        // is an env-var name (e.g. closing `"` in `FOO="bar"`).  The next non-whitespace
        // word token will then reset current_command_range to None so that it can be
        // recognised as a fresh command word.
        let mut assignment_value_just_closed = false;
        let mut cursor_token_idx = None;

        let mut idx = 0;
        while idx < self.tokens.len() {
            // When closing an ArithSubst, two consecutive ) tokens are required.
            // Merge them into a single DoubleRParen by modifying self.tokens[idx] in place
            // and removing the second ) from the vector.
            if matches!(
                nestings.last().map(|(_, k)| k),
                Some(TokenKind::ArithSubst | TokenKind::ArithCommand)
            ) && self.tokens[idx].token.kind == TokenKind::RParen
                && idx + 1 < self.tokens.len()
                && self.tokens[idx + 1].token.kind == TokenKind::RParen
            {
                let second = self.tokens.remove(idx + 1);
                self.tokens[idx].token.value.push_str(&second.token.value);
                self.tokens[idx].token.kind = TokenKind::DoubleRParen;
            }

            // If the previous env-var value nesting just closed, reset the command range
            // now (before the arg-merging check below) so that the next word token is
            // treated as the start of a fresh command rather than as another argument to
            // the assignment statement.  The reset is deferred until here so that it skips
            // over any intervening whitespace tokens.  For non-word, non-whitespace tokens
            // (e.g. redirects) the flag is cleared without resetting the range.
            if assignment_value_just_closed {
                if self.tokens[idx].token.kind.is_word() {
                    self.current_command_range = None;
                }
                if !matches!(self.tokens[idx].token.kind, TokenKind::Whitespace(_)) {
                    assignment_value_just_closed = false;
                }
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

            let previous_kind = previous_token.as_ref().map(|t| &t.token.kind);
            let in_plain_word_context = self.current_command_range.is_some()
                || previous_kind
                    .is_some_and(|kind| matches!(kind, TokenKind::Assignment | TokenKind::Dollar))
                || self
                    .tokens
                    .get(idx + 1)
                    .is_some_and(|next| next.token.kind == TokenKind::Assignment);

            let should_normalize_reserved_token = match self.tokens[idx].token.kind {
                TokenKind::If
                | TokenKind::Case
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until => in_plain_word_context,
                TokenKind::In => !Self::is_in_header_context(&self.tokens, &nestings, idx),
                TokenKind::Fi | TokenKind::Done | TokenKind::Esac => {
                    !Self::nested_closing_satisfied(
                        &self.tokens[idx].token,
                        nestings.last().map(|(_, k)| k),
                    )
                }
                _ => Self::is_builtin_like_reserved_word(&self.tokens[idx].token.kind),
            };

            if should_normalize_reserved_token {
                self.tokens[idx].token.kind = TokenKind::Word(self.tokens[idx].token.value.clone());
            }

            // Clone the token so we can match on it while still mutating self.tokens[idx].annotation.
            let token = self.tokens[idx].token.clone();
            let active_heredoc_opening_idx =
                Self::active_heredoc_opening_idx(&self.tokens, &heredocs, &nestings, &token);

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
            if token_inclusively_contains_cursor && cursor_token_idx.is_none() {
                cursor_token_idx = Some(idx);
            }

            let in_nesting_after_cursor = nestings.last().is_some_and(|(open_idx, _)| {
                cursor_token_idx.is_some_and(|c_idx| *open_idx > c_idx)
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
                | TokenKind::LParen
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
                    let depth = nestings.len();
                    self.tokens[idx].annotations.opening = Some(OpeningState::Unmatched);

                    if matches!(
                        token.kind,
                        TokenKind::If
                            | TokenKind::Case
                            | TokenKind::For
                            | TokenKind::While
                            | TokenKind::Until
                    ) {
                        // Do not color keywords like bracket tokens
                    } else {
                        self.tokens[idx].annotations.bracket_depth = Some(depth);
                    }

                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    }
                    nestings.push((idx, token.kind.clone()));
                    command_start_stack.push(self.current_command_range.clone());
                    self.current_command_range = None; // set for next word after this
                }
                TokenKind::HereDoc { delimiter, quoted }
                | TokenKind::HereDocDash { delimiter, quoted } => {
                    let depth = nestings.len();
                    self.tokens[idx].annotations.opening = Some(OpeningState::Unmatched);
                    self.tokens[idx].annotations.bracket_depth = Some(depth);

                    heredocs.push_back((idx, delimiter.clone(), *quoted, depth));
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
                    let depth = nestings.len();
                    self.tokens[idx].annotations.closing = Some(ClosingAnnotation {
                        opening_idx,
                        is_auto_inserted: false,
                    });

                    if matches!(
                        token.kind,
                        TokenKind::Fi | TokenKind::Done | TokenKind::Esac
                    ) {
                        // Do not color keywords like bracket tokens
                    } else {
                        self.tokens[idx].annotations.bracket_depth = Some(depth);
                    }

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

                    let is_nesting_before_or_at_cursor =
                        cursor_token_idx.map_or(true, |c_idx| opening_idx <= c_idx);
                    if stop_parsing_at_command_boundary
                        && !cursor_part_way_through_token
                        && current_command_range_contains_cursor
                        && is_nesting_before_or_at_cursor
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

                    // If the restored range begins with an env-var token (e.g. the `FOO` in
                    // `FOO="bar"`), the nesting we just closed was the value of an env-var
                    // assignment.  The next word token should start a fresh command, so defer
                    // the reset of current_command_range until then.
                    if self
                        .current_command_range
                        .as_ref()
                        .and_then(|r| self.tokens.get(*r.start()))
                        .is_some_and(|t| t.annotations.is_env_var)
                    {
                        assignment_value_just_closed = true;
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

                    if (stop_parsing_at_command_boundary && !in_nesting_after_cursor)
                        || token_inclusively_contains_cursor
                    {
                        break;
                    }
                    self.current_command_range = None;
                }
                TokenKind::Word(word)
                    if active_heredoc_opening_idx.is_some()
                        && heredocs
                            .front()
                            .is_some_and(|(_, delim, _quoted, _depth)| delim == word) =>
                {
                    let (opening_idx, _, _, depth) = heredocs.pop_front().unwrap();
                    self.tokens[idx].annotations.closing = Some(ClosingAnnotation {
                        opening_idx,
                        is_auto_inserted: false,
                    });
                    self.tokens[idx].annotations.bracket_depth = Some(depth);
                }
                TokenKind::In => {
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }

                // Redirection operators (`<`, `>`, `>>`, `<&`, `>&`, `<>`, `>|`).
                // They never act as a command word and never start a new command;
                // they just extend the current command range if one exists.
                TokenKind::Less
                | TokenKind::Great
                | TokenKind::DGreat
                | TokenKind::InputDup
                | TokenKind::OutputDup
                | TokenKind::ReadWrite
                | TokenKind::Clobber => {
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
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
                    if stop_parsing_at_command_boundary && !in_nesting_after_cursor {
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
                        let last_nesting_should_single_quote_idx = nestings
                            .last()
                            .map(|(idx, k)| (*idx, *k == TokenKind::SingleQuote));
                        let cur_heredoc_is_quoted_idx =
                            if let Some(active_idx) = active_heredoc_opening_idx {
                                heredocs
                                    .front()
                                    .filter(|(_, _, quoted, _)| *quoted)
                                    .map(|_| active_idx)
                            } else {
                                None
                            };
                        match (
                            last_nesting_should_single_quote_idx,
                            cur_heredoc_is_quoted_idx,
                        ) {
                            (Some((nesting_idx, should_single_quote)), Some(heredoc_idx)) => {
                                nesting_idx > heredoc_idx && should_single_quote
                            }
                            (Some((_, should_single_quote)), None) => should_single_quote,
                            (None, Some(_)) => true,
                            (None, None) => false,
                        }
                    };
                    let in_double_quote = {
                        let last_nesting_should_double_quote_idx = nestings
                            .last()
                            .map(|(idx, k)| (*idx, *k == TokenKind::Quote));
                        let cur_heredoc_is_unquoted_idx =
                            if let Some(active_idx) = active_heredoc_opening_idx {
                                heredocs
                                    .front()
                                    .filter(|(_, _, quoted, _)| !*quoted)
                                    .map(|_| active_idx)
                            } else {
                                None
                            };
                        match (
                            last_nesting_should_double_quote_idx,
                            cur_heredoc_is_unquoted_idx,
                        ) {
                            (Some((nesting_idx, should_double_quote)), Some(heredoc_idx)) => {
                                nesting_idx > heredoc_idx && should_double_quote
                            }
                            (Some((_, should_double_quote)), None) => should_double_quote,
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

    fn is_inside_matched_quote(
        tokens: &[AnnotatedToken],
        opener_kind: TokenKind,
        byte_pos: usize,
    ) -> bool {
        tokens.iter().any(|t| {
            if let Some(OpeningState::Matched(close_idx)) = t.annotations.opening {
                if t.token.kind == opener_kind {
                    let open_end = t.token.byte_range().end;
                    let close_start = tokens[close_idx].token.byte_range().start;
                    return open_end <= byte_pos && byte_pos <= close_start;
                }
            }
            false
        })
    }

    /// Returns `true` if `byte_pos` falls inside a matched `$(...)` or backtick
    /// command-substitution. `$()` and backticks reset the quoting context, so
    /// callers can use this to decide that the outer double-quote restriction
    /// does not apply.
    fn is_inside_cmdsubst_or_backtick(tokens: &[AnnotatedToken], byte_pos: usize) -> bool {
        tokens.iter().any(|t| {
            if let Some(OpeningState::Matched(close_idx)) = t.annotations.opening {
                if matches!(t.token.kind, TokenKind::CmdSubst | TokenKind::Backtick) {
                    let open_end = t.token.byte_range().end;
                    let close_start = tokens[close_idx].token.byte_range().start;
                    return open_end <= byte_pos && byte_pos <= close_start;
                }
            }
            false
        })
    }

    pub fn consume_overwritten_auto_inserted_closing(
        tokens: &mut [AnnotatedToken],
        c: char,
        cursor_pos: usize,
    ) -> bool {
        if cursor_pos == 0 {
            return false;
        }

        if let Some(dparser_token) = tokens
            .iter_mut()
            .find(|t| t.token.byte_range().contains(&cursor_pos))
            && let Some(closing) = dparser_token.annotations.closing.as_mut()
            && closing.is_auto_inserted
            && dparser_token.token.value.starts_with(c)
        {
            closing.is_auto_inserted = false;
            return true;
        }

        false
    }

    pub fn should_delete_auto_inserted_closing(
        tokens: &[AnnotatedToken],
        cursor_pos: usize,
    ) -> bool {
        if cursor_pos == 0 {
            return false;
        }

        let Some(opening_token) = tokens
            .iter()
            .find(|t| t.token.byte_range().contains(&(cursor_pos - 1)))
        else {
            return false;
        };

        if let Some(OpeningState::Matched(closing_idx)) = opening_token.annotations.opening {
            return tokens.get(closing_idx).is_some_and(|closing_token| {
                let is_empty_arith_command_pair = opening_token.token.kind
                    == TokenKind::ArithCommand
                    && opening_token.token.byte_range().end == cursor_pos
                    && matches!(
                        closing_token.token.kind,
                        TokenKind::DoubleRParen | TokenKind::RParen
                    )
                    && closing_token.token.value.starts_with(')');

                closing_token.token.byte_range().start == cursor_pos
                    && (closing_token
                        .annotations
                        .closing
                        .as_ref()
                        .is_some_and(|closing| closing.is_auto_inserted)
                        || is_empty_arith_command_pair)
            });
        }

        // Fallback for lexer/parser edge cases where no structural opening/closing annotation
        // is available (e.g. consecutive `(` merged into ArithCommand tokens). If the cursor is
        // exactly between an opening char and an auto-inserted matching closing token, delete it.
        let Some(opening_char) = opening_token.token.value.chars().next_back() else {
            return false;
        };
        let Some(expected_closing_char) = surround_closing_char(opening_char) else {
            return false;
        };

        opening_token.token.byte_range().end == cursor_pos
            && tokens.iter().any(|closing_token| {
                let is_empty_arith_command_pair = opening_token.token.kind
                    == TokenKind::ArithCommand
                    && matches!(
                        closing_token.token.kind,
                        TokenKind::DoubleRParen | TokenKind::RParen
                    )
                    && closing_token.token.value.starts_with(')');

                closing_token.token.byte_range().start == cursor_pos
                    && closing_token.token.value.starts_with(expected_closing_char)
                    && (closing_token
                        .annotations
                        .closing
                        .as_ref()
                        .is_some_and(|closing| closing.is_auto_inserted)
                        || is_empty_arith_command_pair)
            })
    }

    pub fn mark_auto_inserted_closing(
        tokens: &mut [AnnotatedToken],
        c: char,
        byte_pos: usize,
    ) -> bool {
        for token in tokens {
            if token.token.byte_range().start == byte_pos && token.token.value.starts_with(c) {
                if let Some(closing) = &mut token.annotations.closing {
                    closing.is_auto_inserted = true;
                } else {
                    // The token kind is not paired by the dparser nesting machinery
                    // (e.g. `]`, which is intentionally not a nesting closer because
                    // `[` does not start a nesting). Synthesize a closing annotation
                    // purely so the auto-close machinery can recognise this token
                    // as auto-inserted on the next keystroke. `opening_idx` is unused
                    // for non-nested closers and is set to 0 as a placeholder.
                    token.annotations.closing = Some(ClosingAnnotation {
                        opening_idx: 0,
                        is_auto_inserted: true,
                    });
                }
                return true;
            }
        }

        false
    }

    /// Returns the closing character that should be automatically inserted after the character `c`
    /// was typed at byte position `just_inserted_pos`.
    ///
    /// `tokens` must come from parsing the buffer *after* `c` was inserted. This lets the caller
    /// ask dparser whether the freshly inserted character is acting as a new opener or as a
    /// closer in the current parse state.
    pub fn closing_char_to_insert_after_insertion(
        tokens: &[AnnotatedToken],
        c: char,
        just_inserted_pos: usize,
    ) -> Option<char> {
        let Some(default_closing) = surround_closing_char(c) else {
            return None;
        };

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

        let is_inside_single_quote =
            Self::is_inside_matched_quote(tokens, TokenKind::SingleQuote, just_inserted_pos);
        let is_inside_double_quote =
            Self::is_inside_matched_quote(tokens, TokenKind::Quote, just_inserted_pos);
        let is_inside_cmdsubst_or_backtick =
            Self::is_inside_cmdsubst_or_backtick(tokens, just_inserted_pos);
        let inserted_end = just_inserted_pos + c.len_utf8();

        // If a word token begins immediately after the inserted character, we inserted the quote
        // right before or in the middle of an existing word. Auto-closing would wrap only an
        // empty string and leave that word outside the quotes.
        let is_before_word = tokens.iter().any(|t| {
            t.token.kind.is_word()
                && t.token.byte_range().start == inserted_end
                && t.token
                    .value
                    .chars()
                    .next()
                    .is_some_and(|ch| !ch.is_whitespace())
        });

        // Inside a single-quoted string nothing is special – no auto-closing at all.
        if is_inside_single_quote {
            return None;
        }

        // Inside a double-quoted string, auto-closing is limited:
        //   • $( → )  and  ${ → }  are always allowed (expansions active inside "...").
        //   • When inside a $() or backtick nested inside the double quote, the quoting
        //     context is reset, so all normal auto-closing rules apply (fall through).
        //   • Everything else is suppressed.
        if is_inside_double_quote && !is_inside_cmdsubst_or_backtick {
            if !is_before_word && matches!(c, '(' | '{') {
                let token_at_insertion = tokens
                    .iter()
                    .find(|t| t.token.byte_range().contains(&just_inserted_pos))
                    .map(|t| &t.token.kind);
                let closing = match (c, token_at_insertion) {
                    ('(', Some(TokenKind::CmdSubst)) => Some(')'),
                    ('{', Some(TokenKind::ParamExpansion)) => Some('}'),
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

        // `[` in command position is the POSIX `[ ... ]` test command — the user
        // will type `[ expr ]` themselves, so an auto-inserted `]` is in the way.
        // Detect this by checking whether the freshly inserted `[` parsed as an
        // `LBracket` token annotated as a command word.
        if c == '[' {
            let inserted_at_command_position = tokens.iter().any(|t| {
                t.token.byte_range().start == just_inserted_pos
                    && matches!(t.token.kind, TokenKind::LBracket)
                    && t.annotations.command_word.is_some()
            });
            if inserted_at_command_position {
                return None;
            }
        }

        // Unambiguously opening characters – always auto-close.
        if matches!(c, '{' | '[' | '(') {
            return Some(default_closing);
        }

        // Ambiguous characters: if the inserted token is acting as a closer in the current parse,
        // don't auto-insert another copy.
        let inserted_token_is_closing = tokens.iter().any(|token| {
            token.token.byte_range().start == just_inserted_pos
                && token.token.value.starts_with(c)
                && token.annotations.closing.is_some()
        });

        if inserted_token_is_closing {
            None
        } else {
            Some(default_closing)
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
        let input = "cat <<A <<-\\B\nline1\nA\nline2\nB\n";
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
        assert_eq!(tokens[4].token.value, "<<-\\B");
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

        // These ones had a heredoc that was quoted in some way
        // So the heredoc body should not be expanded.
        // So I treat it like a single quoted string.
        assert_eq!(tokens[9].token.value, "\n");
        assert_eq!(
            tokens[9].annotations,
            Annotations::default().with_is_inside_single_quotes()
        );
        assert_eq!(tokens[10].token.value, "line2");
        assert_eq!(
            tokens[10].annotations,
            Annotations::default().with_is_inside_single_quotes()
        );
        assert_eq!(tokens[11].token.value, "\n");
        assert_eq!(
            tokens[11].annotations,
            Annotations::default().with_is_inside_single_quotes()
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
    fn test_arithmetic_nested_bracket_annotations() {
        let input = "echo $(( ((2) + 2) ))";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // Expected tokens:
        // 0: "echo"
        // 1: " "
        // 2: "$((", kind: ArithSubst, opening matching with the final "))"
        // 3: " "
        // 4: "(", kind: LParen, matches with 12: ")"
        // 5: "(", kind: LParen, matches with 7: ")"
        // 6: "2"
        // 7: ")", kind: RParen, matches with 5
        // 8: " "
        // 9: "+"
        // 10: " "
        // 11: "2"
        // 12: ")", kind: RParen, matches with 4
        // 13: " "
        // 14: "))", kind: DoubleRParen, matches with 2

        assert_eq!(tokens[2].token.kind, TokenKind::ArithSubst);
        assert_eq!(
            tokens[2].annotations.opening,
            Some(OpeningState::Matched(14))
        );

        assert_eq!(tokens[4].token.kind, TokenKind::LParen);
        assert_eq!(
            tokens[4].annotations.opening,
            Some(OpeningState::Matched(12))
        );

        assert_eq!(tokens[5].token.kind, TokenKind::LParen);
        assert_eq!(
            tokens[5].annotations.opening,
            Some(OpeningState::Matched(7))
        );

        assert_eq!(tokens[7].token.kind, TokenKind::RParen);
        assert_eq!(
            tokens[7].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 5,
                is_auto_inserted: false
            })
        );

        assert_eq!(tokens[12].token.kind, TokenKind::RParen);
        assert_eq!(
            tokens[12].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 4,
                is_auto_inserted: false
            })
        );

        assert_eq!(tokens[14].token.kind, TokenKind::DoubleRParen);
        assert_eq!(
            tokens[14].annotations.closing,
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
    fn test_quoted_assignment_value_followed_by_command() {
        // `ASD="123" foo`: `ASD` is the env-var name, `"123"` is a quoted value,
        // and `foo` is the command that should receive the command_word annotation.
        let input = r#"ASD="123" foo"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // ASD – the variable name, marked as env var
        assert_eq!(tokens[0].token.value, "ASD");
        assert!(tokens[0].annotations.is_env_var);
        assert_eq!(tokens[0].annotations.command_word, None);

        // = – the assignment operator
        assert_eq!(tokens[1].token.kind, TokenKind::Assignment);

        // " – opening double-quote for the value
        assert_eq!(tokens[2].token.value, "\"");

        // 123 – the quoted value, inside double quotes
        assert_eq!(tokens[3].token.value, "123");
        assert!(tokens[3].annotations.is_inside_double_quotes);

        // " – closing double-quote
        assert_eq!(tokens[4].token.value, "\"");

        // foo – this is the command word
        assert_eq!(tokens[6].token.value, "foo");
        assert_eq!(tokens[6].annotations.command_word, Some("foo".to_string()));
    }

    #[test]
    fn test_multiple_quoted_assignments_then_command() {
        // `A="1" B="2" cmd`: two quoted env-var assignments, then `cmd` as the command word.
        let input = r#"A="1" B="2" cmd"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // A – first env var name
        assert_eq!(tokens[0].token.value, "A");
        assert!(tokens[0].annotations.is_env_var);
        assert_eq!(tokens[0].annotations.command_word, None);

        // B – second env var name
        let b_idx = tokens.iter().position(|t| t.token.value == "B").unwrap();
        assert!(tokens[b_idx].annotations.is_env_var);
        assert_eq!(tokens[b_idx].annotations.command_word, None);

        // cmd – the command word
        let cmd_idx = tokens.iter().position(|t| t.token.value == "cmd").unwrap();
        assert_eq!(
            tokens[cmd_idx].annotations.command_word,
            Some("cmd".to_string())
        );
    }

    #[test]
    fn test_single_quoted_assignment_value_followed_by_command() {
        // `ASD='123' foo`: single-quoted value, then `foo` as the command word.
        let input = "ASD='123' foo";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotations);
        }

        // ASD – env var name
        assert_eq!(tokens[0].token.value, "ASD");
        assert!(tokens[0].annotations.is_env_var);
        assert_eq!(tokens[0].annotations.command_word, None);

        // foo – the command word
        let foo_idx = tokens.iter().position(|t| t.token.value == "foo").unwrap();
        assert_eq!(
            tokens[foo_idx].annotations.command_word,
            Some("foo".to_string())
        );
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

        // `in` – reserved keyword in `for` header.
        assert_eq!(tokens[4].token.kind, TokenKind::In);
        assert_eq!(tokens[4].token.value, "in");
        assert_eq!(tokens[4].annotations.command_word, None);

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

    #[test]
    fn test_reserved_tokens_are_words_when_used_as_arguments() {
        let input = "echo if fi done case in break continue return export complete";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));

        for word in [
            "if", "fi", "done", "case", "in", "break", "continue", "return", "export", "complete",
        ] {
            let idx = tokens.iter().position(|t| t.token.value == word).unwrap();
            assert_eq!(tokens[idx].token.kind, TokenKind::Word(word.to_string()));
            assert_eq!(tokens[idx].annotations.opening, None);
            assert_eq!(tokens[idx].annotations.closing, None);
            assert_eq!(tokens[idx].annotations.command_word, None);
        }

        assert_eq!(parser.get_current_command_str(), input);
        assert!(!parser.needs_more_input());
    }

    #[test]
    fn test_if_and_fi_stay_reserved_when_used_as_keywords() {
        let input = "if true; then echo hi; fi";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        let if_idx = tokens.iter().position(|t| t.token.value == "if").unwrap();
        let fi_idx = tokens.iter().position(|t| t.token.value == "fi").unwrap();

        assert_eq!(tokens[if_idx].token.kind, TokenKind::If);
        assert_eq!(
            tokens[if_idx].annotations.opening,
            Some(OpeningState::Matched(fi_idx))
        );
        assert_eq!(tokens[fi_idx].token.kind, TokenKind::Fi);
        assert_eq!(
            tokens[fi_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: if_idx,
                is_auto_inserted: false
            })
        );
        assert!(!parser.needs_more_input());
    }

    #[test]
    fn test_case_done_and_esac_stay_reserved_when_used_as_keywords() {
        let input = "case x in x) echo ok ;; esac";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        let case_idx = tokens.iter().position(|t| t.token.value == "case").unwrap();
        let in_idx = tokens.iter().position(|t| t.token.value == "in").unwrap();
        let esac_idx = tokens.iter().position(|t| t.token.value == "esac").unwrap();

        assert_eq!(tokens[case_idx].token.kind, TokenKind::Case);
        assert_eq!(
            tokens[case_idx].annotations.opening,
            Some(OpeningState::Matched(esac_idx))
        );
        assert_eq!(tokens[in_idx].token.kind, TokenKind::In);
        assert_eq!(tokens[in_idx].annotations.command_word, None);
        assert_eq!(tokens[esac_idx].token.kind, TokenKind::Esac);
        assert_eq!(
            tokens[esac_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: case_idx,
                is_auto_inserted: false
            })
        );
        assert!(!parser.needs_more_input());
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

    // ---- redirection annotation tests ----

    /// `foo 2>&1 bar`: `foo` is the command word, the redirection (`2`, `>&`, `1`)
    /// and the trailing `bar` argument all sit inside that command's range, and
    /// none of the redirect tokens become a command word of their own.
    #[test]
    fn test_redirect_2_and_1_does_not_break_command_word() {
        let input = "foo 2>&1 bar";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].token.value, "foo");
        assert_eq!(tokens[0].annotations.command_word, Some("foo".to_string()));
        // `2`, `>&`, `1`, `bar` should all have no command_word annotation.
        for t in &tokens[1..] {
            assert_eq!(t.annotations.command_word, None);
        }

        // The whole pipeline should be in one command range.
        assert_eq!(parser.get_current_command_str(), "foo 2>&1 bar",);
    }

    /// `>&` must never act as a command word, even if no command precedes it.
    #[test]
    fn test_leading_redirect_op_is_not_command_word() {
        for input in [
            "> out cmd",
            ">> out cmd",
            "< in cmd",
            ">& 2 cmd",
            "<& 0 cmd",
            "<> rw cmd",
            ">| out cmd",
        ] {
            let tokens = DParser::parse_and_annotate(input);
            for t in &tokens {
                if matches!(
                    t.token.kind,
                    TokenKind::Less
                        | TokenKind::Great
                        | TokenKind::DGreat
                        | TokenKind::InputDup
                        | TokenKind::OutputDup
                        | TokenKind::ReadWrite
                        | TokenKind::Clobber
                ) {
                    assert!(
                        t.annotations.command_word.is_none(),
                        "redirect op {:?} in {input:?} should not be tagged as command_word",
                        t.token.value
                    );
                }
            }
        }
    }

    /// In `ls | foo 2>&1 bar`, both `ls` and `foo` are command words; the
    /// redirect tokens never are.
    #[test]
    fn test_redirect_after_pipe_keeps_command_word() {
        let input = "ls | foo 2>&1 bar";
        let tokens = DParser::parse_and_annotate(input);

        let cmd_words: Vec<_> = tokens
            .iter()
            .filter_map(|t| t.annotations.command_word.as_deref())
            .collect();
        assert_eq!(cmd_words, vec!["ls", "foo"]);
    }

    /// `cat <input >output` — the redirects must not become command words and
    /// the surrounding tokens must not be silently dropped from `cat`'s range.
    #[test]
    fn test_input_and_output_redirect_in_same_command() {
        let input = "cat <input >output";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].annotations.command_word, Some("cat".to_string()));
        for t in &tokens[1..] {
            assert_eq!(t.annotations.command_word, None);
        }
        assert_eq!(parser.get_current_command_str(), input);
    }

    #[test]
    fn test_heredoc_operator_before_pipe_does_not_mark_pipeline_as_body() {
        let input = "cat <<EOF | sort";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        let pipe_idx = tokens
            .iter()
            .position(|t| t.token.kind == TokenKind::Pipe)
            .unwrap();
        let sort_idx = tokens.iter().position(|t| t.token.value == "sort").unwrap();

        assert!(!tokens[pipe_idx].annotations.is_inside_double_quotes);
        assert!(!tokens[sort_idx].annotations.is_inside_double_quotes);
        assert!(!tokens[sort_idx].annotations.is_inside_single_quotes);
        assert_eq!(tokens[sort_idx].annotations.closing, None);
        assert!(parser.needs_more_input());
    }

    #[test]
    fn test_heredoc_operator_before_pipe_with_open_quote_stays_normal_code() {
        let input = "cat <<EOF | echo \"\nhi\"";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        let pipe_idx = tokens
            .iter()
            .position(|t| t.token.kind == TokenKind::Pipe)
            .unwrap();
        let echo_idx = tokens.iter().position(|t| t.token.value == "echo").unwrap();

        assert!(!tokens[pipe_idx].annotations.is_inside_double_quotes);
        assert!(!tokens[echo_idx].annotations.is_inside_double_quotes);
        assert!(!tokens[echo_idx].annotations.is_inside_single_quotes);
        assert!(parser.needs_more_input());
    }

    // ---- [ / ] lexer-token tests (after flash upgrade) ----

    /// After the flash upgrade, a bare `[` is a dedicated `LBracket` token
    /// (no longer `Word("[")`) and `]` is a dedicated `RBracket` token. The
    /// dparser must keep working with both.
    #[test]
    fn test_single_brackets_are_lbracket_rbracket() {
        let tokens = collect_tokens_include_whitespace("[ -f x ]");
        assert_eq!(tokens[0].kind, TokenKind::LBracket);
        assert_eq!(tokens[0].value, "[");
        // ` `, `-f`, ` `, `x`, ` `, `]`
        assert_eq!(tokens.last().unwrap().kind, TokenKind::RBracket);
        assert_eq!(tokens.last().unwrap().value, "]");
    }

    /// `[` is never a nesting opener — neither at command position nor as
    /// an argument. `[ foo` is a complete command (the POSIX `[` builtin
    /// will run and complain at runtime). The opening `[` is annotated as
    /// the command word.
    #[test]
    fn test_single_bracket_is_not_a_nesting_opener() {
        let input = "[ foo";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].token.kind, TokenKind::LBracket);
        assert_eq!(tokens[0].annotations.command_word, Some("[".to_string()));
        assert_eq!(tokens[0].annotations.opening, None);
        assert!(!parser.needs_more_input());
    }

    /// `[` after a command word is not a nesting opener either: it's just a
    /// regular argument. `echo [ grep ]` has `echo` as the only command
    /// word; `[`, `grep` and `]` are arguments with no command_word
    /// annotation and no opening/closing annotations.
    #[test]
    fn test_single_bracket_after_command_is_argument_only() {
        let input = "echo [ grep ]";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].annotations.command_word, Some("echo".to_string()));
        for t in &tokens[1..] {
            assert_eq!(t.annotations.command_word, None);
            assert_eq!(t.annotations.opening, None);
            assert_eq!(t.annotations.closing, None);
        }
        assert!(!parser.needs_more_input());
        assert_eq!(parser.get_current_command_str(), input);
    }

    /// `[[` (DoubleLBracket) IS a nesting opener and must be matched with `]]`.
    #[test]
    fn test_double_bracket_is_a_nesting_opener() {
        let input = "[[ 1 == 1 ]]";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        assert_eq!(tokens[0].token.kind, TokenKind::DoubleLBracket);
        let last_idx = tokens.len() - 1;
        assert_eq!(tokens[last_idx].token.kind, TokenKind::DoubleRBracket);
        assert_eq!(
            tokens[0].annotations.opening,
            Some(OpeningState::Matched(last_idx))
        );
        assert_eq!(
            tokens[last_idx].annotations.closing,
            Some(ClosingAnnotation {
                opening_idx: 0,
                is_auto_inserted: false
            })
        );
    }

    /// Unclosed `[[` leaves the parser asking for more input.
    #[test]
    fn test_unclosed_double_bracket_needs_more_input() {
        let mut parser = DParser::from("[[ 1 == 1");
        parser.walk_to_end();
        assert!(parser.needs_more_input());
    }

    // ---- bracket_depth annotation tests ----

    /// Outermost bracket has depth 0; a nested bracket inside it has depth 1.
    #[test]
    fn test_bracket_depth_nested() {
        let input = "echo $(echo $(true))";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        // Find the outer $( token.
        let outer_open = tokens
            .iter()
            .find(|t| t.token.kind == TokenKind::CmdSubst && t.annotations.bracket_depth == Some(0))
            .expect("outer $( not found");
        // The find predicate already asserts depth == Some(0); use the binding to avoid unused-var warning.
        let _ = outer_open;

        // Find the inner $( token.
        let inner_open = tokens
            .iter()
            .find(|t| t.token.kind == TokenKind::CmdSubst && t.annotations.bracket_depth == Some(1))
            .expect("inner $( not found");
        let _ = inner_open;
    }

    /// Closing tokens carry the same depth as their matching opener.
    #[test]
    fn test_bracket_depth_closing_matches_opening() {
        let input = "f() { echo \"hello\" ; }";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();

        // LBrace opener (depth 0) and RBrace closer (depth 0).
        let lbrace = tokens
            .iter()
            .find(|t| t.token.kind == TokenKind::LBrace)
            .expect("{ not found");
        assert_eq!(lbrace.annotations.bracket_depth, Some(0));

        let rbrace = tokens
            .iter()
            .find(|t| t.token.kind == TokenKind::RBrace)
            .expect("} not found");
        assert_eq!(rbrace.annotations.bracket_depth, Some(0));

        // The inner Quote tokens (depth 1).
        let quotes: Vec<_> = tokens
            .iter()
            .filter(|t| t.token.kind == TokenKind::Quote)
            .collect();
        assert_eq!(quotes.len(), 2);
        for q in &quotes {
            assert_eq!(q.annotations.bracket_depth, Some(1));
        }
    }

    /// Non-delimiter tokens never have a bracket_depth set.
    #[test]
    fn test_non_delimiter_tokens_have_no_bracket_depth() {
        let input = "echo hello";
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        for token in parser.tokens() {
            assert_eq!(token.annotations.bracket_depth, None);
        }
    }
}

#[cfg(test)]
mod test_keyword_bracket_color {
    use super::*;

    #[test]
    fn test_keywords_have_no_bracket_depth() {
        let mut parser = DParser::from("if true; then echo hi; while true; do echo; done; fi");
        parser.walk_to_end();
        for token in parser.tokens() {
            if matches!(
                token.token.kind,
                TokenKind::If | TokenKind::Fi | TokenKind::While | TokenKind::Done
            ) {
                assert_eq!(token.annotations.bracket_depth, None);
            }
        }
    }
}
