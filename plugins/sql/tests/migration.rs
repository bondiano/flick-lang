//! The code `sql migration` blocks stand for: the first the list of them
//! all, each the tables it creates, one that creates none nothing.

use flick_core::plugin::testing::{check, expand};
use flick_plugin_sql::Sql;

const SOURCE: &str = "module App.Schema exposing (..)

import Sql

sql migration 1 \"people\" {
  create table people (id bigint primary key generated always as identity, name text not null);
}

sql migration 2 \"pets\" {
  create table pets (owner bigint not null references people, name text);
  alter table people add column born date;
}

sql migration 3 \"late\" {
  drop table pets;
}
";

#[test]
fn migrations_are_one_list_and_the_tables_they_leave() {
    let code: Vec<String> = (expand(&Sql, SOURCE).into_iter())
        .map(|c| c.unwrap_or_else(|errors| panic!("{errors:?}")))
        .collect();
    insta::assert_snapshot!(code.join("\n----\n"));
}

#[test]
fn a_migration_that_declares_nothing_still_says_what_does_not_apply() {
    let source = SOURCE.replace("drop table pets", "drop table owners");
    let found = expand(&Sql, &source);
    assert_eq!(
        found[2],
        Err(vec!["Q013 `owners`: no table `owners` here".to_owned()]),
        "{found:?}"
    );
    assert!(check(&Sql, &source).is_empty());
}
