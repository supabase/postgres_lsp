use std::ops::Range;

use cstree::text::{TextRange, TextSize};

use super::statement_start::{is_at_stmt_start, TokenStatement, STATEMENT_START_TOKEN_MAPS};
use crate::codegen::SyntaxKind;
use crate::parse::libpg_query_node::libpg_query_node;
use crate::Parser;

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
            let root = result
                .protobuf
                .nodes()
                .iter()
                .find(|n| n.1 == 1)
                .unwrap()
                .0
                .to_enum();

            // FIXME: if have no idea why the subtraction is needed
            let start = if parser.tokens[token_range.start].span.start() == TextSize::from(0) {
                TextSize::from(0)
            } else {
                parser.tokens[token_range.start].span.start() - TextSize::from(1)
            };
            let end = if token_range.end == parser.tokens.len() {
                parser.tokens[token_range.end - 1].span.end() - TextSize::from(1)
            } else {
                parser.tokens[token_range.end - 1].span.end()
            };
            let text_range = TextRange::new(
                TextSize::from(u32::try_from(start).unwrap()),
                TextSize::from(u32::try_from(end).unwrap()),
            );

            parser.stmt(root.clone(), text_range);
            libpg_query_node(parser, root, &token_range);
        }
        Err(err) => {
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

pub fn collect_statement_token_range(parser: &mut Parser, kind: SyntaxKind) -> Range<usize> {
    parser.open_buffer();

    // advance with all start tokens of statement
    advance_over_start_tokens(parser, kind);

    let mut is_sub_stmt = 0;
    let mut is_sub_trx = 0;
    let mut ignore_next_non_whitespace = false;
    while !parser.at(SyntaxKind::Ascii59) && !parser.eof() {
        match parser.nth(0, false).kind {
            SyntaxKind::All => {
                // ALL is never a statement start, but needs to be skipped when combining queries
                // (e.g. UNION ALL)
                parser.advance();
            }
            SyntaxKind::BeginP => {
                // BEGIN, consume until END
                is_sub_trx += 1;
                parser.advance();
            }
            SyntaxKind::EndP => {
                is_sub_trx -= 1;
                parser.advance();
            }
            // opening brackets "(", consume until closing bracket ")"
            SyntaxKind::Ascii40 => {
                is_sub_stmt += 1;
                parser.advance();
            }
            SyntaxKind::Ascii41 => {
                is_sub_stmt -= 1;
                parser.advance();
            }
            SyntaxKind::As | SyntaxKind::Union | SyntaxKind::Intersect | SyntaxKind::Except => {
                // ignore the next non-whitespace token
                ignore_next_non_whitespace = true;
                parser.advance();
            }
            _ => {
                // if another stmt FIRST is encountered, break
                // ignore if parsing sub stmt
                if ignore_next_non_whitespace == false
                    && is_sub_stmt == 0
                    && is_sub_trx == 0
                    && is_at_stmt_start(parser).is_some()
                {
                    break;
                } else {
                    if ignore_next_non_whitespace == true && !parser.at_whitespace() {
                        ignore_next_non_whitespace = false;
                    }
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
        if let Some(result) = STATEMENT_START_TOKEN_MAPS[i].get(&token.kind) {
            let is_in_results = result
                .iter()
                .find(|x| match x {
                    TokenStatement::EoS(y) | TokenStatement::Any(y) => y == &kind,
                })
                .is_some();
            if i == 0 && !is_in_results {
                panic!("Expected statement start");
            } else if is_in_results {
                parser.expect(token.kind);
            } else {
                break;
            }
        }
    }
}
