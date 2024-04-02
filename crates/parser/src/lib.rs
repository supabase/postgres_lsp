//! The Postgres parser.
//!
//! This crate provides a parser for the Postgres SQL dialect.
//! It is based in the pg_query.rs crate, which is a wrapper around the PostgreSQL query parser.
//! The main `Parser` struct parses a source file and individual statements.
//! The `Parse` result struct contains the resulting concrete syntax tree, syntax errors, and the abtract syntax tree, which is a list of pg_query statements and their positions.
//!
//! The idea is to offload the heavy lifting to the same parser that the PostgreSQL server uses,
//! and just fill in the gaps to be able to build both cst and ast from a source file that
//! potentially contains erroneous statements.
//!
//! The main drawbacks of the PostgreSQL query parser mitigated by this parser are:
//! - it only parsed a full source text, and if there is any syntax error in a file, it will not parse anything and return an error.
//! - it does not parse whitespaces and newlines, and it only returns ast nodes. The concrete syntax tree has to be reverse-engineered.
//!
//! To see how these drawbacks are mitigated, see the `statement_parser.rs` and the `source_parser.rs` module.

#![feature(lazy_cell, is_sorted)]

mod ast_node;
mod codegen;
mod lexer;
mod parse;
mod parser;
mod sibling_token;
mod syntax_error;
mod syntax_node;

use lexer::lex;
use parse::source::source;
use parse::statement::collect_statement_token_range;
use parse::statement_start::is_at_stmt_start;
use text_size::TextRange;

pub use crate::codegen::SyntaxKind;
pub use crate::parser::{Parse, Parser};
pub use crate::syntax_node::{SyntaxElement, SyntaxNode, SyntaxToken};

// TODO: I think we should add some kind of `EntryPoint` enum and make the api more flexible
// maybe have an intermediate struct that takes &str inputs, lexes the input and then calls the parser
pub fn parse_source(text: &str) -> Parse {
    let mut p = Parser::new(lex(text));
    source(&mut p);
    p.finish()
}

pub fn get_statements(text: &str) -> Vec<(TextRange, String)> {
    let mut parser = Parser::new(lex(text));
    parser.start_node(SyntaxKind::SourceFile);

    let mut ranges = vec![];

    while !parser.eof() {
        match is_at_stmt_start(&mut parser) {
            Some(stmt) => {
                let range = collect_statement_token_range(&mut parser, stmt);

                let from = parser.tokens.get(range.start);
                let to = parser.tokens.get(range.end - 1);
                // get text range from token range
                let start = from.unwrap().span.start();
                let end = to.unwrap().span.end();
                ranges.push((
                    TextRange::new(
                        text_size::TextSize::from(u32::from(start)),
                        text_size::TextSize::from(u32::from(end)),
                    ),
                    text.get(start.into()..end.into()).unwrap().to_string(),
                ));

                while parser.pos < range.end {
                    parser.advance();
                }
            }
            None => {
                parser.advance();
            }
        }
    }

    parser.finish_node();

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_get_statements() {
        init();

        let input = "select 1;   \n select 2; \n select 3;";

        let ranges = get_statements(input);

        println!("{:?}", ranges);

        assert_eq!(ranges.len(), 3);

        assert_eq!(
            input.get(ranges[0].0.start().into()..ranges[0].0.end().into()),
            Some("select 1;")
        );

        assert_eq!(
            input.get(ranges[1].0.start().into()..ranges[1].0.end().into()),
            Some("select 2;")
        );

        assert_eq!(
            input.get(ranges[2].0.start().into()..ranges[2].0.end().into()),
            Some("select 3;")
        );
    }
}
