use conch_parser::lexer::Lexer;

#[test]
fn test_conch_lexer() {
    let input = "echo $(ls \\\n-la)"; // considers dash not as part of the word, which is an issue
    // let input = "echo foo\\ asdf"; // fails
    let tokens: Vec<_> = Lexer::new(input.chars()).collect();
    println!("{:#?}", tokens);
}