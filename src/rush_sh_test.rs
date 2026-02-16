



use rush_sh::lexer::{lex, Token};
use rush_sh::state::ShellState;



#[test]
fn test_oak_lexer() {
    let state = ShellState::default();
    // let toks = lex(r#"echo asdf\ hello "#, &state).unwrap();
    // let toks = lex(r#"echo \"foo"#, &state).unwrap(); 
    // let toks = lex(r#"echo "\"foo""#, &state).unwrap();
    // let toks = lex(r#"echo asdf\ "#, &state).unwrap();
    let toks = lex("echo \t  \t asdf", &state).unwrap();

    for tok in toks {
        println!("{:?}", tok);
    }
}