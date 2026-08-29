/*
 * Bash lexer tests for flyline.
 *
 * Originally derived from flash (https://github.com/raphamorim/flash).
 * Copyright (c) 2025 Raphael Amorim
 *
 * Licensed under GNU General Public License v3.0.
 */

use super::lexer::{Lexer, TokenKind};

#[test]
fn test_lexer_basic_tokens() {
    let mut lexer = Lexer::new("echo hello");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_operators() {
    let mut lexer = Lexer::new("| && || ; & < > >>");

    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::And);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Or);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Semicolon);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Background);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Less);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Great);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::DGreat);
}

#[test]
fn test_lexer_quotes() {
    let mut lexer = Lexer::new(r#""hello world" 'single quoted'"#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello world".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("single quoted".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
}

#[test]
fn test_lexer_variable_expansion() {
    let mut lexer = Lexer::new("$HOME ${USER} $1 $@");

    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("HOME".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("USER".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("@".to_string()));
}

#[test]
fn test_lexer_command_substitution() {
    let mut lexer = Lexer::new("$(echo hello) `date`");

    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Backtick);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("date".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Backtick);
}

#[test]
fn test_lexer_arithmetic_expansion() {
    let mut lexer = Lexer::new("$((1 + 2))");

    assert_eq!(lexer.next_token().kind, TokenKind::ArithSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("+".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
}

#[test]
fn test_lexer_keywords() {
    let mut lexer = Lexer::new("if then elif else fi for while until do done function case esac");

    assert_eq!(lexer.next_token().kind, TokenKind::If);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Then);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Elif);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Else);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Fi);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::For);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::While);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Until);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("do".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("done".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Function);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Case);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Esac);
}

#[test]
fn test_do_is_word_outside_loop_header() {
    let mut lexer = Lexer::new("cd do");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("cd".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("do".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_do_is_keyword_in_loop_header() {
    let mut lexer = Lexer::new("for i in 1; do echo $i; done");

    assert_eq!(lexer.next_token().kind, TokenKind::For);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("i".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::In);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Semicolon);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Do);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("i".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Semicolon);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Done);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_comments() {
    let mut lexer = Lexer::new("echo hello # this is a comment\necho world");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Comment);
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("world".to_string())
    );
}

#[test]
fn test_lexer_hash_not_comment_in_word() {
    let mut lexer = Lexer::new("echo foo && clear# hi");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::And);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("clear#".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("hi".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_newlines() {
    let mut lexer = Lexer::new("echo\n\nworld");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("world".to_string())
    );
}

#[test]
fn test_lexer_braces() {
    let mut lexer = Lexer::new("{ echo hello; }");

    assert_eq!(lexer.next_token().kind, TokenKind::LBrace);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Semicolon);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
}

#[test]
fn test_lexer_comma_brace_expansion_tokens() {
    let mut lexer = Lexer::new("echo {1,2}");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1,2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
}

#[test]
fn test_lexer_parentheses() {
    let mut lexer = Lexer::new("(echo hello)");

    assert_eq!(lexer.next_token().kind, TokenKind::LParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
}

#[test]
fn test_lexer_assignment() {
    let mut lexer = Lexer::new("VAR=value");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("VAR".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Assignment);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("value".to_string())
    );
}

#[test]
fn test_lexer_extended_glob() {
    let mut lexer = Lexer::new("?(pattern) *(pattern) +(pattern) @(pattern) !(pattern)");

    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('?'));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);

    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('*'));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);

    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('+'));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);

    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('@'));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);

    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('!'));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
}

#[test]
fn test_lexer_extended_glob_with_pipes() {
    let mut lexer = Lexer::new("@(foo|bar|baz)");

    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('@'));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("bar".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("baz".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
}

#[test]
fn test_lexer_nested_extended_globs() {
    let mut lexer = Lexer::new("@(a|b|!(c|d))");

    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('@'));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("a".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("b".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(lexer.next_token().kind, TokenKind::ExtGlob('!'));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("c".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Pipe);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("d".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
}

#[test]
fn test_lexer_braces_in_word() {
    let mut lexer = Lexer::new("echo ./foo/{a,b}");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("./foo/".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("a,b".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
}

#[test]
fn test_lexer_unclosed_brace_in_word() {
    let mut lexer = Lexer::new("echo ./foo/{");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("./foo/".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBrace);
}

#[test]
fn test_lexer_double_semicolon() {
    let mut lexer = Lexer::new("case $var in pattern) echo hello ;; esac");

    assert_eq!(lexer.next_token().kind, TokenKind::Case);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("var".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::In);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::DoubleSemicolon);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Esac);
}

#[test]
fn test_lexer_position_tracking() {
    let mut lexer = Lexer::new("echo\nhello");

    let token1 = lexer.next_token();
    assert_eq!(token1.position.line, 1);
    assert_eq!(token1.position.column, 1);

    let token2 = lexer.next_token(); // newline
    assert_eq!(token2.position.line, 1);
    assert_eq!(token2.position.column, 5);

    let token3 = lexer.next_token();
    assert_eq!(token3.position.line, 2);
    assert_eq!(token3.position.column, 1);
}

#[test]
fn test_lexer_whitespace_handling() {
    let mut lexer = Lexer::new("  echo   hello  ");

    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace("  ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace("   ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace("  ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_empty_input() {
    let mut lexer = Lexer::new("");
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_only_whitespace() {
    let mut lexer = Lexer::new("   \t  \n  ");
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace("   \t  ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace("  ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_mixed_quotes() {
    let mut lexer = Lexer::new(r#"echo "hello 'world'" 'goodbye "friend"'"#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello 'world'".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("goodbye \"friend\"".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
}

#[test]
fn test_lexer_escape_sequences() {
    let mut lexer = Lexer::new(r#"echo \$HOME \n \t \\"#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("\\$HOME".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("\\n".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("\\t".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("\\\\".to_string()));
}

#[test]
fn test_lexer_complex_redirection() {
    let mut lexer = Lexer::new("cmd 2>&1 3< file 4>> log");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("cmd".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::OutputDup);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("3".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Less);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("file".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("4".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::DGreat);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("log".to_string()));
}

#[test]
fn test_lexer_single_token_redirection_operators() {
    let mut lexer = Lexer::new(">& <& <> >|");

    assert_eq!(lexer.next_token().kind, TokenKind::OutputDup);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::InputDup);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ReadWrite);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Clobber);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_lexer_glob_patterns() {
    let mut lexer = Lexer::new("ls *.txt [abc] {1,2,3}");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("ls".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("*.txt".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBracket);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("abc".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBracket);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBrace);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("1,2,3".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
}
#[test]
fn test_arith_command_token() {
    let mut lexer = Lexer::new("((");
    let token = lexer.next_token();

    println!("Token: {token:?}");

    // Check if it's an ArithCommand token
    match token.kind {
        TokenKind::ArithCommand => {
            println!("SUCCESS: Got ArithCommand token");
            assert_eq!(token.value, "((");
        }
        _ => {
            println!("FAILURE: Expected ArithCommand, got {:?}", token.kind);
            panic!("Expected ArithCommand token");
        }
    }
}

#[test]
fn test_arith_command_full() {
    let mut lexer = Lexer::new("(( 5 == 10 ))");

    println!("All tokens:");
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        println!("{token:?}");

        if token.kind == TokenKind::EOF {
            tokens.push(token);
            break;
        }
        tokens.push(token);
    }

    // Check that the first token is ArithCommand
    assert_eq!(tokens[0].kind, TokenKind::ArithCommand);
    assert_eq!(tokens[0].value, "((");
}

#[test]
fn test_gte_tokens() {
    let mut lexer = Lexer::new("(( 3 >= 5 ))");

    println!("All tokens:");
    loop {
        let token = lexer.next_token();
        println!("{token:?}");

        if token.kind == TokenKind::EOF {
            break;
        }
    }
}

#[test]
fn test_nested_arithmetic_tokens() {
    let mut lexer = Lexer::new("(( $((5 + 3)) > 7 ))");

    println!("All tokens for nested arithmetic:");
    loop {
        let token = lexer.next_token();
        println!("{token:?}");

        if token.kind == TokenKind::EOF {
            break;
        }
    }
}

#[test]
fn test_lexer_arithmetic_nested_parentheses() {
    let mut lexer = Lexer::new("$(( ((2) + 2) ))");
    assert_eq!(lexer.next_token().kind, TokenKind::ArithSubst);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LParen);
    assert_eq!(lexer.next_token().kind, TokenKind::LParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("+".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_deeply_nested_tokens() {
    let mut lexer = Lexer::new("(( $((1 + $((2 * 3)))) == 7 ))");

    println!("All tokens for deeply nested:");
    loop {
        let token = lexer.next_token();
        println!("{token:?}");

        if token.kind == TokenKind::EOF {
            break;
        }
    }
}

#[test]
fn test_double_quote_dollar_expansion() {
    // echo "hello $FOO" should lex out the dollar sign and FOO
    let mut lexer = Lexer::new(r#"echo "hello $FOO""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_dollar_at_start() {
    // "$FOO" - dollar at the start of a double-quoted string
    let mut lexer = Lexer::new(r#""$FOO""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_param_expansion() {
    // "${FOO}" inside double quotes
    let mut lexer = Lexer::new(r#""${FOO}""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_param_expansion_with_text() {
    // "hello ${FOO} world" - param expansion with surrounding text
    let mut lexer = Lexer::new(r#""hello ${FOO} world""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word(" world".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_cmd_substitution_with_text() {
    // "result: $(echo hello)" - command substitution with leading text
    let mut lexer = Lexer::new(r#""result: $(echo hello)""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("result: ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("hello".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_nested_cmd_substitution() {
    let mut lexer = Lexer::new(r#"echo "$( foo1 $(bar) )" && baz"#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("bar".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::And);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("baz".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_nested_cmd_substitution_with_surrounding_text() {
    let mut lexer = Lexer::new(r#"echo "prefix $( foo1 $(bar) ) suffix""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("prefix ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("bar".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word(" suffix".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_nested_arith_substitution_in_cmd_substitution() {
    let mut lexer = Lexer::new(r#"echo "$( foo1 $((1+2)) )" && baz"#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ArithSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1+2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::And);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("baz".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_nested_arith_substitution_with_surrounding_text() {
    let mut lexer = Lexer::new(r#"echo "prefix $( foo1 $((1+2)) ) suffix""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("prefix ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::CmdSubst);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("foo1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::ArithSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1+2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word(" suffix".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_arith_substitution() {
    // "$((1 + 2))" - arithmetic substitution inside double quotes
    let mut lexer = Lexer::new(r#""$((1 + 2))""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::ArithSubst);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("1".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("+".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("2".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::RParen);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_backtick_substitution() {
    // "`date`" inside double quotes - backtick command substitution
    let mut lexer = Lexer::new(r#""`date`""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::Backtick);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("date".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Backtick);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_quote_escaped_dollar() {
    // "\$FOO" - escaped dollar should be literal
    let mut lexer = Lexer::new(r#""\$FOO""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    // \$ is an escape sequence - backslash and $ are literal
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("\\$FOO".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_single_quote_no_expansion() {
    // Single quotes should NOT expand $, backticks, etc.
    let mut lexer = Lexer::new(r#"'$FOO'"#);

    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("$FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_double_rparen_token_kind() {
    // DoubleRParen is defined for downstream dependents and is not emitted by the lexer.
    // Verify the variant exists and its derived traits work correctly.
    let token = TokenKind::DoubleRParen;
    assert_eq!(token, TokenKind::DoubleRParen);
    assert_ne!(token, TokenKind::RParen);
    let cloned = token.clone();
    assert_eq!(cloned, TokenKind::DoubleRParen);
    assert!(format!("{:?}", token).contains("DoubleRParen"));
}

#[test]
fn test_bracket_followed_by_single_quote() {
    // "a ['" should parse as: word, whitespace, lbracket, singlequote
    let mut lexer = Lexer::new("a ['");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("a".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBracket);
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_bracket_followed_by_double_quote() {
    // "a [\"" should parse as: word, whitespace, lbracket, quote
    let mut lexer = Lexer::new("a [\"");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("a".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBracket);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_empty_brackets_are_separate_tokens() {
    let mut lexer = Lexer::new("echo []");

    assert_eq!(lexer.next_token().kind, TokenKind::Word("echo".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Whitespace(" ".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::LBracket);
    assert_eq!(lexer.next_token().kind, TokenKind::RBracket);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_length() {
    // ${#parameter} - length of parameter
    let mut lexer = Lexer::new("${#FOO}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("#".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_prefix_removal() {
    // ${parameter#word} - remove smallest prefix matching word
    let mut lexer = Lexer::new("${FOO#prefix}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("#".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("prefix".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_prefix_removal_longest() {
    // ${parameter##word} - remove largest prefix matching word
    let mut lexer = Lexer::new("${FOO##prefix}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("##".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("prefix".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_substitution() {
    // ${parameter/pattern/string} - replace first occurrence of pattern with string
    let mut lexer = Lexer::new("${FOO/pattern/string}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("string".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_global_substitution() {
    // ${parameter//pattern/string} - replace all occurrences of pattern with string
    let mut lexer = Lexer::new("${FOO//pattern/string}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("//".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("string".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_anchored_prefix_substitution() {
    // ${parameter/#pattern/string} - replace pattern anchored at start
    let mut lexer = Lexer::new("${FOO/#pattern/string}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("#".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("string".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_anchored_suffix_substitution() {
    // ${parameter/%pattern/string} - replace pattern anchored at end
    let mut lexer = Lexer::new("${FOO/%pattern/string}");

    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("%".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("string".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_param_expansion_anchored_prefix_substitution_double_quoted() {
    // "${parameter/#pattern/string}" - same but double-quoted
    let mut lexer = Lexer::new(r#""${FOO/#pattern/string}""#);

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::ParamExpansion);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("#".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("pattern".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/".to_string()));
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("string".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::RBrace);
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_dollar_env_var_path() {
    // $HOME/foo should lex as three tokens: $, HOME, /foo
    let mut lexer = Lexer::new("$HOME/foo");

    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("HOME".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word("/foo".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_dollar_env_var_dot() {
    // $HOME.FOO should lex as three tokens: $, HOME, .FOO
    let mut lexer = Lexer::new("$HOME.FOO");

    assert_eq!(lexer.next_token().kind, TokenKind::Dollar);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("HOME".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Word(".FOO".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

// -------------------------------------------------------------------------
// Invariant: `Word` tokens must never contain a newline character.
//
// In bash a `Word` is a sequence of characters considered as a single unit
// by the shell. Newlines either terminate a command or are line continuations
// inside quoted strings; in neither case should the literal `\n` end up
// inside the lexer's `Word(String)` token. The tests below assert this
// invariant for several constructions where it could plausibly leak through.
// -------------------------------------------------------------------------

/// Walk the lexer to completion and ensure no `Word` token contains '\n'.
fn assert_no_newlines_in_words(input: &str) {
    let mut lexer = Lexer::new(input);
    loop {
        let token = lexer.next_token();
        if let TokenKind::Word(ref w) = token.kind {
            assert!(
                !w.contains('\n'),
                "Word token contained a newline for input {:?}: {:?}",
                input,
                w,
            );
        }
        if matches!(token.kind, TokenKind::EOF) {
            break;
        }
    }
}

#[test]
fn test_word_token_no_newline_unquoted_command() {
    assert_no_newlines_in_words("foo\nbar\nbaz");
}

#[test]
fn test_word_token_no_newline_with_backslash_continuation() {
    // The literal source string here is `foo\<newline>bar`, i.e. a
    // backslash immediately followed by an actual newline character.
    assert_no_newlines_in_words("foo\\\nbar");
}

#[test]
fn test_word_token_no_newline_in_double_quoted_string() {
    assert_no_newlines_in_words("echo \"line1\nline2\"");
}

#[test]
fn test_word_token_no_newline_in_single_quoted_string() {
    assert_no_newlines_in_words("echo 'line1\nline2'");
}

#[test]
fn test_word_token_no_newline_multi_line_double_quoted_with_expansion() {
    assert_no_newlines_in_words("echo \"hello $USER\nbye\"");
}

#[test]
fn test_word_token_no_newline_in_heredoc_body() {
    assert_no_newlines_in_words("cat <<EOF\nhello\nworld\nEOF\n");
}

#[test]
fn test_word_token_no_newline_in_quoted_heredoc_body() {
    assert_no_newlines_in_words("cat <<'EOF'\nhello $USER\nEOF\n");
}

#[test]
fn test_double_quoted_multiline_splits_into_words_and_newlines() {
    let mut lexer = Lexer::new("\"line1\nline2\nline3\"");

    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("line1".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("line2".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(
        lexer.next_token().kind,
        TokenKind::Word("line3".to_string())
    );
    assert_eq!(lexer.next_token().kind, TokenKind::Quote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

#[test]
fn test_single_quoted_multiline_splits_into_words_and_newlines() {
    let mut lexer = Lexer::new("'a\nb'");

    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("a".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::Newline);
    assert_eq!(lexer.next_token().kind, TokenKind::Word("b".to_string()));
    assert_eq!(lexer.next_token().kind, TokenKind::SingleQuote);
    assert_eq!(lexer.next_token().kind, TokenKind::EOF);
}

// -------------------------------------------------------------------------
// Here-document lexing.
//
// From the bash manual:
//
//   If any part of word is quoted, the delimiter is the result of quote
//   removal on word, and the lines in the here-document are not expanded.
//   If word is unquoted, delimiter is word itself, and the here-document
//   text is treated similarly to a double-quoted string.
//
// The tests below exercise the lexer's understanding of the heredoc
// "word" / delimiter for the various ways the word can be (partially)
// quoted: bare, single quoted, double quoted, backslash escaped, and
// concatenated mixed quoting.
// -------------------------------------------------------------------------

fn first_heredoc_token(input: &str) -> (TokenKind, String) {
    let mut lexer = Lexer::new(input);
    loop {
        let token = lexer.next_token();
        match &token.kind {
            TokenKind::HereDoc { .. } | TokenKind::HereDocDash { .. } => {
                return (token.kind.clone(), token.value.clone());
            }
            TokenKind::EOF => panic!("no heredoc token in input: {:?}", input),
            _ => continue,
        }
    }
}

#[test]
fn test_heredoc_unquoted_delimiter() {
    let (kind, value) = first_heredoc_token("cat <<EOF\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: false,
        }
    );
    assert_eq!(value, "<<EOF");
}

#[test]
fn test_heredoc_single_quoted_delimiter_strips_quotes() {
    // Per bash, the delimiter used for line matching is the result of
    // quote removal on `word`, so `<<'EOF'` matches a line containing
    // just `EOF`. The `quoted` flag is true so the body is *not*
    // expanded.
    let (kind, value) = first_heredoc_token("cat <<'EOF'\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        }
    );
    // The token value is the verbatim slice of the input.
    assert_eq!(value, "<<'EOF'");
}

#[test]
fn test_heredoc_double_quoted_delimiter_strips_quotes() {
    let (kind, value) = first_heredoc_token("cat <<\"EOF\"\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        }
    );
    assert_eq!(value, "<<\"EOF\"");
}

#[test]
fn test_heredoc_backslash_escaped_delimiter() {
    // `<<\EOF` is equivalent to `<<'EOF'` after quote removal.
    let (kind, value) = first_heredoc_token("cat <<\\EOF\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        }
    );
    assert_eq!(value, "<<\\EOF");
}

#[test]
fn test_heredoc_partially_quoted_delimiter() {
    // The delimiter `EO'F'` after quote removal is `EOF`, and because
    // some part of the word was quoted the body should not be expanded.
    let (kind, value) = first_heredoc_token("cat <<EO'F'\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        }
    );
    assert_eq!(value, "<<EO'F'");
}

#[test]
fn test_heredoc_dash_unquoted_delimiter() {
    let (kind, value) = first_heredoc_token("cat <<-EOF\n\tbody\n\tEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDocDash {
            delimiter: "EOF".to_string(),
            quoted: false,
        }
    );
    assert_eq!(value, "<<-EOF");
}

#[test]
fn test_heredoc_dash_single_quoted_delimiter_strips_quotes() {
    let (kind, value) = first_heredoc_token("cat <<-'EOF'\n\tbody\n\tEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDocDash {
            delimiter: "EOF".to_string(),
            quoted: true,
        }
    );
    assert_eq!(value, "<<-'EOF'");
}

#[test]
fn test_heredoc_delimiter_with_special_chars_preserved() {
    // No quoting -> delimiter is the literal word as-is and `quoted`
    // remains false (body should be expanded).
    let (kind, _) = first_heredoc_token("cat <<END_OF_FILE\nbody\nEND_OF_FILE\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "END_OF_FILE".to_string(),
            quoted: false,
        }
    );
}

#[test]
fn test_heredoc_delimiter_quoted_with_inner_special_chars() {
    // Quoted delimiter may legitimately contain characters that would
    // normally be operators; quote removal yields just the inner text.
    let (kind, _) = first_heredoc_token("cat <<'E$F'\nbody\nE$F\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "E$F".to_string(),
            quoted: true,
        }
    );
}

#[test]
fn test_heredoc_followed_by_command_terminator() {
    // A here-doc word terminates at whitespace; what follows on the same
    // line is lexed as ordinary tokens.
    let mut lexer = Lexer::new("cat <<EOF arg\nbody\nEOF\n");
    // cat
    assert_eq!(lexer.next_token().kind, TokenKind::Word("cat".to_string()));
    // ' '
    assert!(matches!(lexer.next_token().kind, TokenKind::Whitespace(_)));
    // <<EOF
    let hdtok = lexer.next_token();
    assert_eq!(
        hdtok.kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: false,
        }
    );
    // ' '
    assert!(matches!(lexer.next_token().kind, TokenKind::Whitespace(_)));
    // arg
    assert_eq!(lexer.next_token().kind, TokenKind::Word("arg".to_string()));
}

// -------------------------------------------------------------------------
// `quoted` flag: whether or not the heredoc body should be expanded.
//
// In bash, if any part of `word` in `<<word` (or `<<-word`) is quoted —
// using single quotes, double quotes, or a backslash — then parameter,
// command and arithmetic expansion are NOT performed on the lines of the
// here-document. If `word` is entirely unquoted, the body is expanded
// (treated like a double-quoted string).
//
// The lexer encodes that distinction in the `quoted: bool` field on the
// `HereDoc`/`HereDocDash` token kinds. The tests below assert that the
// flag is computed correctly for every form of (un)quoted delimiter.
// -------------------------------------------------------------------------

fn heredoc_quoted_flag(input: &str) -> bool {
    let (kind, _) = first_heredoc_token(input);
    match kind {
        TokenKind::HereDoc { quoted, .. } | TokenKind::HereDocDash { quoted, .. } => quoted,
        _ => unreachable!(),
    }
}

#[test]
fn test_heredoc_body_expanded_for_unquoted_delimiter() {
    // echo <<EOF -> body is expanded, so quoted == false.
    assert!(!heredoc_quoted_flag("echo <<EOF\n$x\nEOF\n"));
}

#[test]
fn test_heredoc_body_not_expanded_for_single_quoted_delimiter() {
    // echo <<'EOF' -> body is NOT expanded, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<'EOF'\n$x\nEOF\n"));
}

#[test]
fn test_heredoc_body_not_expanded_for_double_quoted_delimiter() {
    // echo <<"EOF" -> body is NOT expanded, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<\"EOF\"\n$x\nEOF\n"));
}

#[test]
fn test_heredoc_body_not_expanded_for_backslash_escaped_delimiter() {
    // echo <<\EOF -> body is NOT expanded, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<\\EOF\n$x\nEOF\n"));
}

#[test]
fn test_heredoc_body_not_expanded_for_partially_quoted_delimiter() {
    // echo <<EO'F' -> any quoting suppresses expansion, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<EO'F'\n$x\nEOF\n"));
}

#[test]
fn test_heredoc_dash_body_expanded_for_unquoted_delimiter() {
    // echo <<-EOF -> body is expanded, so quoted == false.
    assert!(!heredoc_quoted_flag("echo <<-EOF\n\t$x\n\tEOF\n"));
}

#[test]
fn test_heredoc_dash_body_not_expanded_for_single_quoted_delimiter() {
    // echo <<-'EOF' -> body is NOT expanded, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<-'EOF'\n\t$x\n\tEOF\n"));
}

#[test]
fn test_heredoc_dash_body_not_expanded_for_double_quoted_delimiter() {
    // echo <<-"EOF" -> body is NOT expanded, so quoted == true.
    assert!(heredoc_quoted_flag("echo <<-\"EOF\"\n\t$x\n\tEOF\n"));
}

// -------------------------------------------------------------------------
// Verbatim reconstruction: chaining all token values together must yield
// the original input. This is the property that motivates `Token.value`
// being the verbatim slice of the underlying buffer (including for
// here-doc delimiter tokens, which previously dropped quote characters).
// -------------------------------------------------------------------------

fn collect_token_values(input: &str) -> String {
    let mut lexer = Lexer::new(input);
    let mut out = String::new();
    loop {
        let token = lexer.next_token();
        if matches!(token.kind, TokenKind::EOF) {
            break;
        }
        out.push_str(&token.value);
    }
    out
}

#[test]
fn test_heredoc_token_values_reconstruct_input_unquoted() {
    let input = "cat <<EOF\nbody\nEOF\n";
    // Stop the reconstruction check at the heredoc delimiter token; the
    // body itself is consumed by the parser, not the lexer, so we only
    // check the prefix containing the operator.
    let prefix = "cat <<EOF";
    assert!(collect_token_values(input).starts_with(prefix));
}

#[test]
fn test_heredoc_token_values_reconstruct_input_single_quoted() {
    let input = "cat <<'EOF'\nbody\nEOF\n";
    let prefix = "cat <<'EOF'";
    assert!(collect_token_values(input).starts_with(prefix));
}

#[test]
fn test_heredoc_token_values_reconstruct_input_dash_double_quoted() {
    let input = "cat <<-\"EOF\"\n\tbody\n\tEOF\n";
    let prefix = "cat <<-\"EOF\"";
    assert!(collect_token_values(input).starts_with(prefix));
}

#[test]
fn test_heredoc_token_value_preserves_whitespace_between_operator_and_delimiter() {
    // The lexer is permissive and accepts whitespace between `<<` (or
    // `<<-`) and the delimiter word. Because `Token.value` must be a
    // verbatim slice of the input, that whitespace has to appear in the
    // token's `value` even though it is not part of the delimiter
    // itself.
    let (kind, value) = first_heredoc_token("cat << EOF\nbody\nEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: false,
        }
    );
    assert_eq!(value, "<< EOF");

    let (kind, value) = first_heredoc_token("cat <<-\tEOF\n\tbody\n\tEOF\n");
    assert_eq!(
        kind,
        TokenKind::HereDocDash {
            delimiter: "EOF".to_string(),
            quoted: false,
        }
    );
    assert_eq!(value, "<<-\tEOF");
}

// -------------------------------------------------------------------------
// Heredoc body lexing depends on whether the delimiter was quoted.
//
// Per the bash spec:
//   * If the delimiter word is unquoted, the lines of the here-document
//     are treated as if they were inside a double-quoted string, so
//     parameter, command and arithmetic substitutions are tokenized.
//   * If any part of the delimiter word is quoted, the lines are treated
//     as if they were inside a single-quoted string: every character is
//     literal, and the entire body line is emitted as a single `Word`
//     token.
//
// The helper below collects the tokens that appear between the `Newline`
// following the heredoc operator and the `Newline` following the
// delimiter line, so each test can make precise assertions about what
// the body is lexed into.
// -------------------------------------------------------------------------

fn collect_body_tokens(input: &str, delim: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(input);
    let mut started = false;
    let mut body = Vec::new();
    loop {
        let tok = lexer.next_token();
        match &tok.kind {
            TokenKind::EOF => break,
            TokenKind::HereDoc { .. } | TokenKind::HereDocDash { .. } => {
                // The next `Newline` marks the start of the body.
                let nl = lexer.next_token();
                assert!(matches!(nl.kind, TokenKind::Newline));
                started = true;
                continue;
            }
            _ if started => {
                // Stop once we hit the delimiter line. A delimiter is
                // always lexed as a `Word(delim)` (possibly preceded by
                // a `\t` whitespace token in the dash variant — drop
                // any trailing whitespace tokens that we mistook for
                // body content).
                if let TokenKind::Word(w) = &tok.kind
                    && w == delim
                {
                    while matches!(body.last(), Some(TokenKind::Whitespace(_))) {
                        body.pop();
                    }
                    break;
                }
                body.push(tok.kind.clone());
            }
            _ => {}
        }
    }
    body
}

#[test]
fn test_unquoted_heredoc_body_lexed_with_arithmetic_expansion() {
    // `<<EOF` is unquoted, so `$((1+2))` in the body must be tokenized
    // as an arithmetic substitution (i.e. `ArithSubst` + the inner
    // expression + two `RParen`s), exactly as it would inside a
    // double-quoted string.
    let body = collect_body_tokens("cat <<EOF\necho $((1+2))\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::ArithSubst,
            TokenKind::Word("1+2".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_lexed_as_single_word() {
    // `<<'EOF'` is quoted, so the entire body line must be returned as
    // one literal `Word` token; no expansion takes place.
    let body = collect_body_tokens("cat <<'EOF'\necho $((1+2))\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("echo $((1+2))".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_unquoted_heredoc_body_lexed_with_parameter_expansion() {
    // `$VAR` in an unquoted heredoc body is split into `Dollar` +
    // `Word("VAR")`, the same way it would be inside a double-quoted
    // string.
    let body = collect_body_tokens("cat <<EOF\nhi $VAR\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("hi".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("VAR".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_keeps_dollar_variable_literal() {
    // `$VAR` in a single-quoted heredoc body is literal.
    let body = collect_body_tokens("cat <<'EOF'\nhi $VAR\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![TokenKind::Word("hi $VAR".to_string()), TokenKind::Newline,],
    );
}

#[test]
fn test_unquoted_heredoc_body_lexed_with_command_substitution() {
    // `$(date)` in an unquoted heredoc body is tokenized as a command
    // substitution.
    let body = collect_body_tokens("cat <<EOF\nhi $(date)\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("hi".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::CmdSubst,
            TokenKind::Word("date".to_string()),
            TokenKind::RParen,
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_keeps_command_substitution_literal() {
    // `$(date)` in a single-quoted heredoc body is literal text.
    let body = collect_body_tokens("cat <<'EOF'\nhi $(date)\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("hi $(date)".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_unquoted_heredoc_body_lexed_with_braced_param_expansion() {
    // `${X}` in an unquoted heredoc body is tokenized as a parameter
    // expansion.
    let body = collect_body_tokens("cat <<EOF\nv=${X}\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("v".to_string()),
            TokenKind::Assignment,
            TokenKind::ParamExpansion,
            TokenKind::Word("X".to_string()),
            TokenKind::RBrace,
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_keeps_braced_param_expansion_literal() {
    let body = collect_body_tokens("cat <<'EOF'\nv=${X}\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![TokenKind::Word("v=${X}".to_string()), TokenKind::Newline,],
    );
}

#[test]
fn test_quoted_heredoc_body_with_double_quoted_delimiter_is_literal() {
    // Double quoting the delimiter (`<<"EOF"`) suppresses expansion in
    // the body, just like single quoting does.
    let body = collect_body_tokens("cat <<\"EOF\"\necho $((1+2))\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("echo $((1+2))".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_with_backslash_delimiter_is_literal() {
    // `<<\EOF` is equivalent to `<<'EOF'` for body-expansion purposes.
    let body = collect_body_tokens("cat <<\\EOF\nhi $X\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![TokenKind::Word("hi $X".to_string()), TokenKind::Newline,],
    );
}

#[test]
fn test_quoted_heredoc_body_partially_quoted_delimiter_is_literal() {
    // Any quoting in the delimiter word suppresses body expansion; the
    // post-quote-removal delimiter is `EOF`.
    let body = collect_body_tokens("cat <<EO'F'\nhi $X\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![TokenKind::Word("hi $X".to_string()), TokenKind::Newline,],
    );
}

#[test]
fn test_quoted_heredoc_body_multiple_lines_each_emitted_as_word() {
    // Each line of a quoted heredoc body is a single `Word`, separated
    // by `Newline` tokens.
    let body = collect_body_tokens("cat <<'EOF'\nline1 $A\nline2 $((1+1))\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Word("line1 $A".to_string()),
            TokenKind::Newline,
            TokenKind::Word("line2 $((1+1))".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_body_empty_line_emits_only_newline() {
    // An empty body line doesn't produce a `Word("")` token; just the
    // bare `Newline`.
    let body = collect_body_tokens("cat <<'EOF'\n\nbody $X\nEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Newline,
            TokenKind::Word("body $X".to_string()),
            TokenKind::Newline,
        ],
    );
}

#[test]
fn test_quoted_heredoc_dash_body_is_literal_and_preserves_leading_tabs() {
    // For `<<-` the leading tab is stripped only when matching the
    // delimiter line; the body line itself is emitted verbatim (with
    // its leading tab) as a single `Word`.
    let body = collect_body_tokens("cat <<-'EOF'\n\thi $X\n\tEOF\n", "EOF");
    assert_eq!(
        body,
        vec![TokenKind::Word("\thi $X".to_string()), TokenKind::Newline,],
    );
}

#[test]
fn test_unquoted_heredoc_dash_body_lexes_expansions() {
    // The dash variant with an unquoted delimiter still expands.
    let body = collect_body_tokens("cat <<-EOF\n\thi $X\n\tEOF\n", "EOF");
    assert_eq!(
        body,
        vec![
            TokenKind::Whitespace("\t".to_string()),
            TokenKind::Word("hi".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("X".to_string()),
            TokenKind::Newline,
        ],
    );
}

// -------------------------------------------------------------------------
// Robustness tests for heredoc delimiter lexing.
//
// The heredoc delimiter "word" supports embedded quoted segments
// (`<<'EOF'`, `<<EO'F'`, `<<"EOF"`, …). When such an embedded segment
// has no matching close quote on the same word, the lexer must NOT
// greedily swallow input that belongs to a separate, unclosed
// quoted-string token following the heredoc operator. Otherwise the
// rest of the script is silently absorbed into the heredoc operator's
// raw value.
//
// These tests cover the primary problem case from the issue
// (`<<'EOF''`) plus a few related shapes (double quotes, fully
// unclosed quotes, unclosed quotes in the middle of the word).
// -------------------------------------------------------------------------

fn collect_all_tokens(input: &str) -> Vec<(TokenKind, String)> {
    let mut lexer = Lexer::new(input);
    let mut out = Vec::new();
    loop {
        let token = lexer.next_token();
        let done = matches!(token.kind, TokenKind::EOF);
        out.push((token.kind, token.value));
        if done {
            break;
        }
    }
    out
}

#[test]
fn test_heredoc_unmatched_trailing_single_quote_left_for_outer_lexer() {
    // `<<'EOF''` — the third single quote has no matching closer on
    // the delimiter word, so it must remain as a separate
    // (unclosed) `SingleQuote` token after the heredoc operator. The
    // heredoc operator's raw value must include only `<<'EOF'` and
    // its delimiter must be `EOF`.
    let tokens = collect_all_tokens("cat <<'EOF''\nfoo\nEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDoc { .. }))
        .expect("expected a HereDoc token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        },
    );
    assert_eq!(heredoc_value, "<<'EOF'");

    // The token immediately following the heredoc operator must be
    // the unmatched single quote (NOT a Newline — that would mean
    // the quote was swallowed into the operator).
    let (next_kind, next_value) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::SingleQuote);
    assert_eq!(next_value, "'");
}

#[test]
fn test_heredoc_unmatched_trailing_double_quote_left_for_outer_lexer() {
    // `<<"EOF""` — analogous to the single-quoted case above.
    let tokens = collect_all_tokens("cat <<\"EOF\"\"\nfoo\nEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDoc { .. }))
        .expect("expected a HereDoc token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDoc {
            delimiter: "EOF".to_string(),
            quoted: true,
        },
    );
    assert_eq!(heredoc_value, "<<\"EOF\"");

    let (next_kind, next_value) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::Quote);
    assert_eq!(next_value, "\"");
}

#[test]
fn test_heredoc_fully_unclosed_single_quoted_delimiter() {
    // `<<'EOF` — the only single quote in the delimiter has no
    // matching closer at all. The opening quote must NOT be eaten
    // by the heredoc operator: it has to remain as the start of an
    // unclosed single-quoted string.
    let tokens = collect_all_tokens("cat <<'EOF\nfoo\nEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDoc { .. }))
        .expect("expected a HereDoc token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    // Empty delimiter, not quoted, raw value is just `<<`.
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDoc {
            delimiter: String::new(),
            quoted: false,
        },
    );
    assert_eq!(heredoc_value, "<<");

    let (next_kind, next_value) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::SingleQuote);
    assert_eq!(next_value, "'");
}

#[test]
fn test_heredoc_fully_unclosed_double_quoted_delimiter() {
    // `<<"EOF` — analogous to the single-quoted case above.
    let tokens = collect_all_tokens("cat <<\"EOF\nfoo\nEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDoc { .. }))
        .expect("expected a HereDoc token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDoc {
            delimiter: String::new(),
            quoted: false,
        },
    );
    assert_eq!(heredoc_value, "<<");

    let (next_kind, next_value) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::Quote);
    assert_eq!(next_value, "\"");
}

#[test]
fn test_heredoc_unclosed_quote_mid_word_truncates_delimiter() {
    // `<<EO'F` — `EO` is bare, then an unmatched `'` opens a
    // quoted segment that is never closed. The delimiter must be
    // truncated to `EO`, and the `'F` must remain for the outer
    // lexer to handle.
    let tokens = collect_all_tokens("cat <<EO'F\nfoo\nEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDoc { .. }))
        .expect("expected a HereDoc token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDoc {
            delimiter: "EO".to_string(),
            quoted: false,
        },
    );
    assert_eq!(heredoc_value, "<<EO");

    let (next_kind, next_value) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::SingleQuote);
    assert_eq!(next_value, "'");
}

#[test]
fn test_heredoc_dash_unmatched_trailing_single_quote_left_for_outer_lexer() {
    // Same robustness check, but for the `<<-` (dash) variant.
    let tokens = collect_all_tokens("cat <<-'EOF''\n\tfoo\n\tEOF\n");

    let heredoc_idx = tokens
        .iter()
        .position(|(k, _)| matches!(k, TokenKind::HereDocDash { .. }))
        .expect("expected a HereDocDash token");
    let (heredoc_kind, heredoc_value) = &tokens[heredoc_idx];
    assert_eq!(
        heredoc_kind,
        &TokenKind::HereDocDash {
            delimiter: "EOF".to_string(),
            quoted: true,
        },
    );
    assert_eq!(heredoc_value, "<<-'EOF'");

    let (next_kind, _) = &tokens[heredoc_idx + 1];
    assert_eq!(next_kind, &TokenKind::SingleQuote);
}

#[test]
fn test_heredoc_unmatched_trailing_quote_problem_statement_full_token_stream() {
    // Verify the full token stream for the exact reproducer from the
    // issue: after the heredoc operator, the third single quote
    // opens an unclosed single-quoted string that absorbs the rest
    // of the input (foo, newlines, EOF) — no closing `'` ever
    // appears, so the lexer should still terminate cleanly at EOF.
    let kinds: Vec<TokenKind> = collect_all_tokens("cat <<'EOF''\nfoo\nEOF\n")
        .into_iter()
        .map(|(k, _)| k)
        .collect();

    assert_eq!(
        kinds,
        vec![
            TokenKind::Word("cat".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::HereDoc {
                delimiter: "EOF".to_string(),
                quoted: true,
            },
            TokenKind::SingleQuote,
            TokenKind::Newline,
            TokenKind::Word("foo".to_string()),
            TokenKind::Newline,
            TokenKind::Word("EOF".to_string()),
            TokenKind::Newline,
            TokenKind::EOF,
        ],
    );
}
