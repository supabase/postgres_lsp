use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

use cstree::text::{TextRange, TextSize};

use super::statement_start::{is_at_stmt_start, TokenStatement, STATEMENT_START_TOKEN_MAPS};
use crate::codegen::SyntaxKind;
use crate::parser::WHITESPACE_TOKENS;
use crate::{lexer, Parser};

pub fn statement(parser: &mut Parser, kind: SyntaxKind) {
    let token_range = collect_statement_token_range(parser, kind);
    let tokens = parser.tokens.get(token_range.clone()).unwrap().to_vec();
    match pg_query::parse(
        tokens
            .iter()
            .map(|t| t.text.clone())
            .collect::<String>()
            .as_str(),
    ) {
        Ok(result) => {
            // TODO: return syntax kind and use it in checkpoint
            self.libpg_query_node(
                result
                    .protobuf
                    .nodes()
                    .iter()
                    .find(|n| n.1 == 1)
                    .unwrap()
                    .0
                    .to_enum(),
                token_range.end,
            );
        }
        Err(err) => {
            println!("error: {}", err);
            parser.error(
                err.to_string(),
                TextRange::new(
                    TextSize::from(u32::try_from(token_range.start).unwrap()),
                    TextSize::from(u32::try_from(token_range.end).unwrap()),
                ),
            );
            while parser.pos < token_range.end {
                parser.advance();
            }
        }
    };

    assert_eq!(parser.pos, token_range.end);
}

fn collect_statement_token_range(parser: &mut Parser, kind: SyntaxKind) -> Range<usize> {
    parser.open_buffer();

    // advance with all start tokens of statement
    advance_over_start_tokens(parser, kind);

    let mut is_parsing_sub_stmt = false;
    while !parser.at(SyntaxKind::Ascii59) && !parser.eof() {
        match parser.nth(0, false) {
            // opening brackets "(", consume until closing bracket ")"
            SyntaxKind::Ascii40 => {
                is_parsing_sub_stmt = true;
                parser.advance();
            }
            SyntaxKind::Ascii41 => {
                is_parsing_sub_stmt = false;
                parser.advance();
            }
            _ => {
                // if another stmt FIRST is encountered, break
                // ignore if parsing sub stmt
                if is_parsing_sub_stmt == false && is_at_stmt_start(parser).is_some() {
                    break;
                } else {
                    parser.advance();
                }
            }
        }
    }

    parser.expect(SyntaxKind::Ascii59);

    // close buffer, get tokens and reset pos
    parser.close_buffer()
}

/// advance with all start tokens of statement
fn advance_over_start_tokens(parser: &mut Parser, kind: SyntaxKind) {
    for i in 0..STATEMENT_START_TOKEN_MAPS.len() {
        parser.eat_whitespace();
        let token = parser.nth(0, false);
        if let Some(result) = STATEMENT_START_TOKEN_MAPS[i].get(&token) {
            let is_in_results = result
                .iter()
                .find(|x| match x {
                    TokenStatement::EoS(y) | TokenStatement::Any(y) => y == &kind,
                })
                .is_some();
            if i == 0 && !is_in_results {
                panic!("Expected statement start");
            } else if is_in_results {
                parser.expect(token);
            } else {
                break;
            }
        }
    }
}

