use codegen::parser_codegen;

parser_codegen!();

#[cfg(test)]
mod tests {
    use log::debug;
    use proptest::prelude::*;

    use crate::codegen::{get_nodes, SyntaxKind, TokenProperty};

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_get_nodes() {
        init();

        let input = "with c as (insert into contact (id) values ('id')) select * from c;";

        let pg_query_root = match pg_query::parse(input) {
            Ok(parsed) => Some(
                parsed
                    .protobuf
                    .nodes()
                    .iter()
                    .find(|n| n.1 == 1)
                    .unwrap()
                    .0
                    .to_enum(),
            ),
            Err(_) => None,
        };

        let node_graph = get_nodes(&pg_query_root.unwrap(), 0);
        assert_eq!(node_graph.node_count(), 13);
    }

    fn test_get_node_properties(input: &str, kind: SyntaxKind, expected: Vec<TokenProperty>) {
        init();

        let pg_query_root = match pg_query::parse(input) {
            Ok(parsed) => Some(
                parsed
                    .protobuf
                    .nodes()
                    .iter()
                    .find(|n| n.1 == 1)
                    .unwrap()
                    .0
                    .to_enum(),
            ),
            Err(_) => None,
        };

        debug!("pg_query_root: {:#?}", pg_query_root);

        let node_graph = get_nodes(&pg_query_root.unwrap(), 0);

        debug!("node graph: {:#?}", node_graph);

        let node_index = node_graph
            .node_indices()
            .find(|n| node_graph[*n].kind == kind)
            .unwrap();

        debug!("selected node: {:#?}", node_graph[node_index]);

        // note: even though we test for strict equality of the two vectors the order
        // of the properties does not have to match the order of the tokens in the string
        assert_eq!(node_graph[node_index].properties, expected);
        assert_eq!(node_graph[node_index].properties.len(), expected.len());
    }

    proptest! {
        #[test]
        fn test_simple_select(n in 0..100i32) {
            test_get_node_properties(&format!("select {};", n), SyntaxKind::SelectStmt, vec![TokenProperty::from(SyntaxKind::Select)])
        }
    }

    proptest! {
        #[test]
        fn test_select_with_from(n in 0..100i32, table_name in "ab?c?d?") {
            test_get_node_properties(
                &format!("select {} from {};", n, table_name),
                SyntaxKind::SelectStmt,
                vec![
                    TokenProperty::from(SyntaxKind::Select),
                    TokenProperty::from(SyntaxKind::From),
                ],
            )
        }
    }

    proptest! {
        #[test]
        fn test_select_with_where(table_name in "ab?c?d?", condition in "<|>|=|<>|!=", n in 0..100i32) {
            test_get_node_properties(
                &format!("select * from {} where id {} {};", table_name, condition, n),
                SyntaxKind::SelectStmt,
                vec![
                    TokenProperty::from(SyntaxKind::Select),
                    TokenProperty::from(SyntaxKind::From),
                    TokenProperty::from(SyntaxKind::Where),
                ],
            )
        }
    }

    #[test]
    fn test_select_with_where() {
        test_get_node_properties(
            "select 1 from contact where id = 1;",
            SyntaxKind::SelectStmt,
            vec![
                TokenProperty::from(SyntaxKind::Select),
                TokenProperty::from(SyntaxKind::From),
                TokenProperty::from(SyntaxKind::Where),
            ],
        )
    }

    #[test]
    fn test_select_with_order_by() {
        test_get_node_properties(
            "SELECT a, b, c FROM table1 ORDER BY c;",
            SyntaxKind::SortBy,
            vec![
                TokenProperty::from(SyntaxKind::Order),
                TokenProperty::from(SyntaxKind::By),
            ],
        )
    }

    #[test]
    fn test_create_domain() {
        test_get_node_properties(
            "create domain us_postal_code as text check (value is not null);",
            SyntaxKind::CreateDomainStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::DomainP),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from("us_postal_code".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_schema() {
        test_get_node_properties(
            "create schema if not exists test authorization joe;",
            SyntaxKind::CreateSchemaStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Schema),
                TokenProperty::from(SyntaxKind::IfP),
                TokenProperty::from(SyntaxKind::Not),
                TokenProperty::from(SyntaxKind::Exists),
                TokenProperty::from(SyntaxKind::Authorization),
                TokenProperty::from("test".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_view() {
        test_get_node_properties(
            "create or replace temporary view comedies as select * from films;",
            SyntaxKind::ViewStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::View),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from(SyntaxKind::Or),
                TokenProperty::from(SyntaxKind::Replace),
                TokenProperty::from(SyntaxKind::Temporary),
            ],
        )
    }

    #[test]
    fn test_create_enum() {
        test_get_node_properties(
            "create type status as enum ('open', 'closed');",
            SyntaxKind::CreateEnumStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::TypeP),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from(SyntaxKind::EnumP),
                TokenProperty::from("status".to_string()),
                TokenProperty::from("open".to_string()),
                TokenProperty::from("closed".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_cast() {
        test_get_node_properties(
            "create cast (bigint as int4) with inout as assignment;",
            SyntaxKind::CreateCastStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Cast),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from(SyntaxKind::With),
                TokenProperty::from(SyntaxKind::Inout),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from(SyntaxKind::Assignment),
            ],
        )
    }

    #[test]
    fn test_create_range() {
        test_get_node_properties(
            "create type type1 as range (subtype = int4);",
            SyntaxKind::CreateRangeStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::TypeP),
                TokenProperty::from(SyntaxKind::As),
                TokenProperty::from(SyntaxKind::Range),
                TokenProperty::from("type1".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_function() {
        test_get_node_properties(
            r#"create function getfoo(int)
                returns setof users
                language sql
                as $$select * from "users" where users.id = $1;$$;
            "#,
            SyntaxKind::CreateFunctionStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Function),
                TokenProperty::from(SyntaxKind::Returns),
                TokenProperty::from(SyntaxKind::Setof),
                TokenProperty::from("getfoo".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_index() {
        test_get_node_properties(
            "create unique index title_idx on films (title);",
            SyntaxKind::IndexStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Unique),
                TokenProperty::from(SyntaxKind::Index),
                TokenProperty::from(SyntaxKind::On),
                TokenProperty::from(SyntaxKind::Using),
                TokenProperty::from("title_idx".to_string()),
                TokenProperty::from("btree".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_procedure() {
        test_get_node_properties(
            "create procedure insert_data(a integer)
                language sql
                as $$insert into tbl values (a);$$;",
            SyntaxKind::CreateFunctionStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Procedure),
                TokenProperty::from("insert_data".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_tablespace() {
        test_get_node_properties(
            "create tablespace x owner a location 'b' with (seq_page_cost=3);",
            SyntaxKind::CreateTableSpaceStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Tablespace),
                TokenProperty::from(SyntaxKind::Location),
                TokenProperty::from(SyntaxKind::Owner),
                TokenProperty::from(SyntaxKind::With),
                TokenProperty::from("x".to_string()),
                TokenProperty::from("b".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_type() {
        test_get_node_properties(
            "create type type1",
            SyntaxKind::DefineStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::TypeP),
                TokenProperty::from("type1".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_composite_type() {
        test_get_node_properties(
            "create type type1 as (attr1 int4, attr2 bool);",
            SyntaxKind::CompositeTypeStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::TypeP),
                TokenProperty::from(SyntaxKind::As),
            ],
        )
    }

    #[test]
    fn test_create_database() {
        test_get_node_properties(
            "create database x owner abc connection limit 5;",
            SyntaxKind::CreatedbStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Database),
                TokenProperty::from("x".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_extension() {
        test_get_node_properties(
            r#"create extension if not exists x cascade version "1.2" schema a;"#,
            SyntaxKind::CreateExtensionStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Extension),
                TokenProperty::from(SyntaxKind::IfP),
                TokenProperty::from(SyntaxKind::Not),
                TokenProperty::from(SyntaxKind::Exists),
                TokenProperty::from("x".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_conversion() {
        test_get_node_properties(
            "CREATE DEFAULT CONVERSION myconv FOR 'UTF8' TO 'LATIN1' FROM myfunc;",
            SyntaxKind::CreateConversionStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Default),
                TokenProperty::from(SyntaxKind::ConversionP),
                TokenProperty::from(SyntaxKind::For),
                TokenProperty::from(SyntaxKind::To),
                TokenProperty::from(SyntaxKind::From),
                TokenProperty::from("utf8".to_string()),
                TokenProperty::from("latin1".to_string()),
                TokenProperty::from("myconv".to_string()),
                TokenProperty::from("myfunc".to_string()),
            ],
        )
    }

    #[test]
    fn test_create_transform() {
        test_get_node_properties(
            "CREATE OR REPLACE TRANSFORM FOR hstore LANGUAGE plpython3u (
                FROM SQL WITH FUNCTION hstore_to_plpython(internal),
                TO SQL WITH FUNCTION plpython_to_hstore(internal)
            );",
            SyntaxKind::CreateTransformStmt,
            vec![
                TokenProperty::from(SyntaxKind::Create),
                TokenProperty::from(SyntaxKind::Or),
                TokenProperty::from(SyntaxKind::Replace),
                TokenProperty::from(SyntaxKind::Transform),
                TokenProperty::from(SyntaxKind::For),
                TokenProperty::from(SyntaxKind::Language),
                TokenProperty::from(SyntaxKind::From),
                TokenProperty::from(SyntaxKind::SqlP),
                TokenProperty::from(SyntaxKind::With),
                TokenProperty::from(SyntaxKind::Function),
                TokenProperty::from(SyntaxKind::To),
                TokenProperty::from(SyntaxKind::SqlP),
                TokenProperty::from(SyntaxKind::With),
                TokenProperty::from(SyntaxKind::Function),
                TokenProperty::from("plpython3u".to_string()),
            ],
        )
    }

    #[test]
    fn test_truncate() {
        test_get_node_properties(
            "TRUNCATE TABLE users CONTINUE IDENTITY RESTRICT",
            SyntaxKind::TruncateStmt,
            vec![
                TokenProperty::from(SyntaxKind::Truncate),
                TokenProperty::from(SyntaxKind::Table),
                TokenProperty::from(SyntaxKind::ContinueP),
                TokenProperty::from(SyntaxKind::IdentityP),
                TokenProperty::from(SyntaxKind::Restrict),
            ],
        )
    }
}
