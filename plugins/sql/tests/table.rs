//! The code `sql table` blocks stand for, and what is wrong with the ones
//! that stand for none.

use flick_core::plugin::testing::expand;
use flick_plugin_sql::Sql;

/// What `block` stands for, the one block of a module.
fn one(block: &str) -> Result<String, Vec<String>> {
    let source = format!("module App.Main exposing (..)\n\nimport Sql\n\n{block}\n");
    let mut found = expand(&Sql, &source);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

#[test]
fn a_table_is_its_row_type_and_its_companion() {
    let code = one(
        "sql table Users \"users\" {\n  id : bigint primary key generated always as UserId via user_id\n  name : text not null as String\n  email : text as String\n}",
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    insta::assert_snapshot!(code);
}

#[test]
fn a_table_of_the_wrong_shape_is_refused() {
    let errors = one("sql table Users {\n  id : bigint as BigInt\n}").expect_err("refused");
    assert_eq!(
        errors,
        ["Q010 on the block: a table is `sql table Name \"table\" { column : type as T }`"]
    );
}
