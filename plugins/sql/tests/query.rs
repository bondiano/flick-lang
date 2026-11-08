//! The code `sql { … }` blocks stand for, and what is wrong with the ones
//! that stand for none.

use flick_core::plugin::testing::expand;
use flick_plugin_sql::Sql;

const TABLES: &str = "module App.Main exposing (..)

import Sql exposing (Pg)

sql table Users \"users\" {
  id : bigint primary key generated always as BigInt
  name : text not null as String
  email : text as String
}

sql table Accounts \"accounts\" {
  id : bigint primary key generated always as BigInt
  user_id : bigint not null as BigInt
  balance : numeric not null as BigDecimal.BigDecimal
}
";

/// What the query `query`, the last block of a module with the tables,
/// stands for.
fn last(query: &str) -> Result<String, Vec<String>> {
    let mut found = expand(&Sql, &format!("{TABLES}\n{query}\n"));
    found.pop().expect("the query")
}

#[test]
fn a_query_is_its_text_and_the_function_of_its_types() {
    let code = last(
        "balances : Sql.Template Pg { user_id : BigInt } { name : String; balance : Option BigDecimal.BigDecimal }\nlet balances =\n  sql params {\n    from user in Users\n    left_join account in Accounts on account.user_id == user.id\n    where user.id == params.user_id && user.name != $(\"x\")\n    select { name = user.name; balance = account.balance }\n  }",
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    insta::assert_snapshot!(code);
}

#[test]
fn a_query_that_breaks_a_rule_of_sql_is_refused_where_it_does() {
    for (query, expected) in [
        (
            "let q = sql { from u in Users; select u.nmae }",
            "Q001 `u.nmae`: `Users` has no column `nmae`",
        ),
        (
            "let q = sql { from u in Users; select v.name }",
            "Q002 `v.name`: no table `v` in this query",
        ),
        (
            "let q = sql { from u in Users; join a in Accounts on a.user_id == u.id; select id }",
            "Q003 `id`: `id` is a column of `u` and `a`: name the alias",
        ),
        (
            "let q = sql { from u in Users; where count u.id > 1; select u.name }",
            "Q006 `count u.id`: an aggregate is computed per group: use it in `select`, `having` or `order_by`",
        ),
        (
            "let q = sql { insert_into Users; values { email = \"e\" } }",
            "Q007 `values { email = \"e\" }`: the insert misses the required column `name`",
        ),
        (
            "let q = sql { from u in Nope; select u.name }",
            "Q010 `Nope`: `Nope` is not a `sql table`",
        ),
        (
            "let q = sql { from u in Users; select { n = ifnull u.name } }",
            "Q010 `ifnull`: `ifnull` is not a SQL function; known: is_null, is_not_null, is_not_distinct_from, coalesce, count, count_all, sum, avg, min, max, exists, is_in",
        ),
    ] {
        let errors = last(query).expect_err(query);
        assert_eq!(errors, [expected], "{query}");
    }
}
