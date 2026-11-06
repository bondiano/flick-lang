use flick_core::plugin::testing::blocks;
use flick_plugin::Span;

use super::schema::{Problem, Table, Write};
use super::{Migration, located, migration, place, schema, type_name};

fn migrations(text: &str) -> Vec<Migration> {
    let blocks = blocks(text);
    (blocks.iter().enumerate())
        .map(|(i, b)| migration(b, i))
        .collect()
}

fn tables(text: &str) -> (Vec<Table>, Vec<Problem>) {
    schema(&migrations(text))
}

const CONDUIT: &str = r#"module App.Schema exposing (..)

import Sql exposing (plugin sql)

sql migration 1 "first" {
  create table users (
    id bigint primary key generated always as identity,
    username text not null unique,
    bio text
  );
  create table follows (
    follower_id bigint not null references users on delete cascade,
    followed_id bigint not null references users (id) on delete cascade,
    primary key (follower_id, followed_id)
  );
  create index follows_by_followed on follows (followed_id desc);
}

sql migration 2 "seen" {
  alter table users add column seen_at text not null default now(), drop column bio;
  alter table users add column motto text not null default "it's me";
  alter table follows rename to subscriptions;
}
"#;

#[test]
fn migrations_build_the_tables_in_order() {
    let (tables, problems) = tables(CONDUIT);
    assert!(problems.is_empty(), "{problems:?}");
    let names: Vec<_> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["users", "subscriptions"]);
    let users: Vec<_> = tables[0]
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.ty.join(" "), c.not_null, c.write()))
        .collect();
    assert_eq!(
        users,
        [
            ("id", "bigint".into(), true, Write::Generated),
            ("username", "text".into(), true, Write::Required),
            ("seen_at", "text".into(), true, Write::Defaulted),
            ("motto", "text".into(), true, Write::Defaulted),
        ]
    );
    assert!(
        tables[1]
            .columns
            .iter()
            .all(|c| c.not_null && c.write() == Write::Required)
    );
    assert_eq!(type_name("article_tags"), "ArticleTags");
}

#[test]
fn a_statement_runs_as_written() {
    let m = migrations(CONDUIT);
    assert_eq!((m[0].version, m[0].name.as_str()), (Some(1), "first"));
    assert_eq!(
        m[0].statements[2].sql(),
        "create index follows_by_followed on follows (followed_id desc)"
    );
    assert_eq!(
        m[1].statements[0].sql(),
        "alter table users add column seen_at text not null default now(), drop column bio"
    );
    assert_eq!(
        m[1].statements[1].sql(),
        "alter table users add column motto text not null default 'it''s me'"
    );
}

#[test]
fn what_does_not_apply_is_a_problem() {
    let text = r#"module App.Schema exposing (..)

import Sql exposing (plugin sql)

sql migration 2 "a" {
  create table posts (author bigint references people, title text);
  create index posts_by_x on posts (x);
  alter table posts drop column body;
  create view v as select 1;
}

sql migration 1 "b" {
  drop table posts;
}
"#;
    let (_, problems) = tables(text);
    let messages: Vec<_> = problems.iter().map(|p| p.message.as_str()).collect();
    assert_eq!(messages[0], "no table `people` to refer to");
    assert_eq!(messages[1], "no column `x` in `posts`");
    assert_eq!(messages[2], "no column `body` in `posts`");
    assert!(
        messages[3].starts_with("`create view` is outside the DDL"),
        "{messages:?}"
    );
    assert_eq!(
        messages[4],
        "migration 1 comes after 2: versions increase in the order written"
    );
    assert_eq!(problems.len(), 5);
}

#[test]
fn a_place_keeps_its_block_and_offset_past_256_blocks_and_16_mib() {
    for (index, start, end) in [
        (256, 3, 9),
        (1, 1 << 24, (1 << 24) + 5),
        (70_000, 0, u32::MAX),
    ] {
        let (back, span) = located((place(index, start), place(index, end)));
        assert_eq!((back, span), (index, Span { start, end }));
    }
}
