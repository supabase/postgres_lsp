use std::{panic::RefUnwindSafe, sync::LazyLock};

use dashmap::DashMap;

use fs::FilePath;
use parser::get_statements;
use schema_cache::SchemaCache;
use text_size::{TextLen, TextRange};
use tree_sitter::{InputEdit, Tree};

use crate::diagnostics::{NotFound, WorkspaceError};

use super::{CloseFileParams, FileChange, FileChangesParams, OpenFileParams, Workspace};

pub(super) struct WorkspaceServer {
    documents: DashMap<FilePath, Document>,
    schema_cache: SchemaCache,
}

pub(crate) struct Statement {
    pub(crate) version: i32,
    parser: tree_sitter::Parser,

    pub(crate) content: String,
    pub(crate) tree_sitter: Tree,
}

impl Statement {
    fn new(content: String) -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(tree_sitter_sql::language())
            .expect("Error loading sql language");

        let tree = parser.parse(&content, None).unwrap();

        Self {
            version: 0,
            parser,
            content,
            tree_sitter: tree,
        }
    }

    fn apply_change(&mut self, change: FileChange) {
        assert!(change.range.is_some());

        let range = change.range.unwrap();

        let edit = edit_from_change(
            &self.content.as_str(),
            usize::from(range.start()),
            usize::from(range.end()),
            change.text.as_str(),
        );

        self.tree_sitter.edit(&edit);

        self.content = apply_change(&self.content, &change);

        self.tree_sitter = self
            .parser
            .parse(&self.content, Some(&self.tree_sitter))
            .unwrap();

        self.version += 1;
    }
}

// we get the version from lsp
pub(crate) struct Document {
    pub(crate) content: String,
    pub(crate) version: i32,

    pub(crate) statements: Vec<(TextRange, Statement)>,
}

impl Document {
    fn new(content: String, version: i32) -> Self {
        Self {
            version,
            statements: get_statements(&content)
                .iter()
                .map(|(range, content)| {
                    let statement = Statement::new(content.clone());
                    (range.clone(), statement)
                })
                .collect(),
            content,
        }
    }

    fn apply_changes(&mut self, changes: FileChangesParams) {
        changes.changes.iter().for_each(|c| {
            self.apply_change(c);
        });
        self.version = changes.version;
    }

    fn apply_change(&mut self, change: &FileChange) {
        if change.range.is_none() {
            self.content = change.text.clone();
            self.statements = get_statements(&self.content)
                .iter()
                .map(|(range, content)| {
                    let statement = Statement::new(content.clone());
                    (range.clone(), statement)
                })
                .collect();
            return;
        }

        self.content = apply_change(&self.content, change);

        if let Some(changed_stmt_pos) = self
            .statements
            .iter()
            .position(|(range, _)| range.contains_range(change.range.unwrap()))
        {
            let stmt_range = self.statements[changed_stmt_pos].0;
            self.statements
                .get_mut(changed_stmt_pos)
                .unwrap()
                .1
                .apply_change(FileChange {
                    text: change.text.clone(),
                    // range must be relative to the start of the statement
                    range: change.range.unwrap().checked_sub(stmt_range.start()),
                });
            self.statements
                .iter_mut()
                .skip_while(|(range, _)| range.start() <= change.range.unwrap().end())
                .for_each(|(range, _)| {
                    if range.start() > stmt_range.end() {
                        if change.is_addition() {
                            range.checked_add(change.diff_size());
                        } else if change.is_deletion() {
                            range.checked_sub(change.diff_size());
                        }
                    }
                });
        } else {
            // remove all statements that are affected by this change,
            let mut min = change.range.unwrap().start();
            let mut max = change.range.unwrap().end();

            let change_range = change.range.unwrap();

            for (range, _) in self.statements.extract_if(|(range, _)| {
                (change_range.start() < range.end()) && (change_range.end() > range.start())
            }) {
                println!("range: {:?}", range);
                if range.start() < min {
                    min = range.start();
                }
                if range.end() > max {
                    max = range.end();
                }
            }
            if self.content.text_len() < max {
                max = self.content.text_len();
            }

            // get text from min(first_stmt_start, change_start) to max(last_stmt_end, change_end)
            let extracted_text = self
                .content
                .as_str()
                .get(usize::from(min)..usize::from(max))
                .unwrap();
            // and reparse
            self.statements
                .extend(
                    get_statements(extracted_text)
                        .iter()
                        .map(|(range, content)| {
                            let statement = Statement::new(content.clone());
                            (range.clone(), statement)
                        }),
                );
        }
    }
}

fn edit_from_change(
    text: &str,
    start_char: usize,
    end_char: usize,
    replacement_text: &str,
) -> InputEdit {
    let mut start_byte = 0;
    let mut end_byte = 0;
    let mut chars_counted = 0;

    let mut line = 0;
    let mut current_line_char_start = 0; // Track start of the current line in characters
    let mut column_start = 0;
    let mut column_end = 0;

    for (idx, c) in text.char_indices() {
        if chars_counted == start_char {
            start_byte = idx;
            column_start = chars_counted - current_line_char_start;
        }
        if chars_counted == end_char {
            end_byte = idx;
            // Calculate column_end based on replacement_text
            let replacement_lines: Vec<&str> = replacement_text.split('\n').collect();
            if replacement_lines.len() > 1 {
                // If replacement text spans multiple lines, adjust line and column_end accordingly
                line += replacement_lines.len() - 1;
                column_end = replacement_lines.last().unwrap().chars().count();
            } else {
                // Single line replacement, adjust column_end based on replacement text length
                column_end = column_start + replacement_text.chars().count();
            }
            break; // Found both start and end
        }
        if c == '\n' {
            line += 1;
            current_line_char_start = chars_counted + 1; // Next character starts a new line
        }
        chars_counted += 1;
    }

    // Adjust end_byte based on the byte length of the replacement text
    if start_byte != end_byte {
        // Ensure there's a range to replace
        end_byte = start_byte + replacement_text.len();
    } else if chars_counted < text.chars().count() && end_char == chars_counted {
        // For insertions at the end of text
        end_byte += replacement_text.len();
    }

    let start_point = tree_sitter::Point::new(line, column_start);
    let end_point = tree_sitter::Point::new(line, column_end);

    // Calculate the new end byte after the insertion
    let new_end_byte = start_byte + replacement_text.len();

    // Calculate the new end position
    let new_lines = replacement_text.matches('\n').count(); // Count how many new lines are in the inserted text
    let last_line_length = replacement_text
        .lines()
        .last()
        .unwrap_or("")
        .chars()
        .count(); // Length of the last line in the insertion

    let new_end_position = if new_lines > 0 {
        // If there are new lines, the row is offset by the number of new lines, and the column is the length of the last line
        tree_sitter::Point::new(start_point.row + new_lines, last_line_length)
    } else {
        // If there are no new lines, the row remains the same, and the column is offset by the length of the insertion
        tree_sitter::Point::new(start_point.row, start_point.column + last_line_length)
    };

    InputEdit {
        start_byte,
        old_end_byte: end_byte,
        new_end_byte,
        start_position: start_point,
        old_end_position: end_point,
        new_end_position,
    }
}

fn apply_change(content: &String, change: &FileChange) -> String {
    if change.range.is_none() {
        return change.text.clone();
    }

    let range = change.range.unwrap();
    let start = usize::from(range.start());
    let end = usize::from(range.end());

    let mut new_content = String::new();
    new_content.push_str(&content[..start]);
    new_content.push_str(&change.text);
    new_content.push_str(&content[end..]);

    new_content
}

/// The `Workspace` object is long-lived, so we want it to be able to cross
/// unwind boundaries.
/// In return, we have to make sure operations on the workspace either do not
/// panic, of that panicking will not result in any broken invariant (it would
/// not result in any undefined behavior as catching an unwind is safe, but it
/// could lead too hard to debug issues)
impl RefUnwindSafe for WorkspaceServer {}

impl Workspace for WorkspaceServer {
    fn open_file(&self, params: OpenFileParams) -> Result<(), WorkspaceError> {
        let OpenFileParams {
            path,
            content,
            version,
        } = params;

        self.documents
            .insert(path.clone(), Document::new(content.clone(), version));

        Ok(())
    }

    fn close_file(&self, params: CloseFileParams) -> Result<(), WorkspaceError> {
        self.documents.remove(&params.path);
        Ok(())
    }

    fn apply_file_changes(&self, params: FileChangesParams) -> Result<(), WorkspaceError> {
        let mut doc = self
            .documents
            .get_mut(&params.path)
            .ok_or_else(|| WorkspaceError::NotFound(NotFound))?;

        doc.apply_changes(params);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fs::FilePath;
    use text_size::{TextRange, TextSize};

    use crate::workspace::{
        server::{Document, Statement},
        FileChange, FileChangesParams,
    };

    #[test]
    fn test_statement_apply_change() {
        let input = "select id  from users;";

        let mut s = Statement::new(input.to_string());

        s.apply_change(FileChange {
            text: ",test".to_string(),
            range: Some(TextRange::new(9.into(), 10.into())),
        });

        assert_eq!(
            &s.tree_sitter
                .root_node()
                .utf8_text(s.content.as_bytes())
                .unwrap(),
            &s.content.as_str()
        );
        assert_eq!(s.content, "select id,test from users;");
    }

    #[test]
    fn test_statement_apply_multiline_change() {
        let input = "select id,\ntest from users;";

        let mut s = Statement::new(input.to_string());

        s.apply_change(FileChange {
            text: "*".to_string(),
            range: Some(TextRange::new(7.into(), 15.into())),
        });

        assert_eq!(
            &s.tree_sitter
                .root_node()
                .utf8_text(s.content.as_bytes())
                .unwrap(),
            &s.content.as_str()
        );
        assert_eq!(s.content, "select * from users;");
    }

    #[test]
    fn test_document_apply_changes() {
        let input = "select id from users;\nselect * from contacts;";

        let mut d = Document::new(input.to_string(), 0);

        assert_eq!(d.statements.len(), 2);

        d.apply_changes(FileChangesParams {
            path: FilePath::new("test.sql"),
            version: 1,
            changes: vec![FileChange {
                text: ",test from users\nselect 1;".to_string(),
                range: Some(TextRange::new(9.into(), 45.into())),
            }],
        });

        assert_eq!("select id,test from users\nselect 1;", d.content);
        assert_eq!(d.statements.len(), 2);
    }

    #[test]
    fn test_document_apply_changes_within_statement() {
        let input = "select id  from users;\nselect * from contacts;";

        let mut d = Document::new(input.to_string(), 0);

        assert_eq!(d.statements.len(), 2);

        d.apply_changes(FileChangesParams {
            path: FilePath::new("test.sql"),
            version: 1,
            changes: vec![FileChange {
                text: ",test".to_string(),
                range: Some(TextRange::new(9.into(), 10.into())),
            }],
        });

        assert_eq!(
            "select id,test from users;\nselect * from contacts;",
            d.content
        );
        assert_eq!(d.statements.len(), 2);
        assert_eq!(
            d.statements
                .iter()
                .find(|s| s.0.start() == TextSize::from(0))
                .unwrap()
                .1
                .version,
            1,
            "should touch the first statement"
        );
        assert_eq!(
            d.statements
                .iter()
                .find(|s| s.0.start() != TextSize::from(0))
                .unwrap()
                .1
                .version,
            0,
            "should not touch the second statement"
        );
    }
}
