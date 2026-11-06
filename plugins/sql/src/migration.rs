//! `sql migration N "name" { DDL }`: the module's tables are the schema its
//! migrations build, in the order written, each with its row type and
//! `Name.table` as `sql table` declares them, and the module has the value
//! `migrations`, the list `Database.Migrate.run` applies. A statement that
//! does not apply is `std/Q013`, one outside the DDL subset `std/Q010`.
//!
//! `collect` builds the schema of each module from all its migrations once;
//! each block then writes the tables whose `create table` it holds, and the
//! first one the list of them all.

use std::collections::BTreeMap;

use flick_plugin::{Arena, Block, Builder, Decl, Diagnostic, Expr, Field, NodeId, Span, Ty};
use serde::{Deserialize, Serialize};

use crate::ddl::{self, located};
use crate::dialect;
use crate::shown::{Named, Shown, ShownColumn};
use crate::table::MALFORMED;

/// A statement that does not apply.
pub const UNAPPLIED: &str = "Q013";

/// What the group's migrations build, by module.
#[derive(Default, Serialize, Deserialize)]
pub struct State {
    modules: BTreeMap<String, Vec<Step>>,
}

/// A migration of a module, in the order written.
#[derive(Serialize, Deserialize)]
struct Step {
    /// The block's text, which tells it apart.
    text: String,
    version: i32,
    name: String,
    statements: Vec<String>,
    /// The tables whose `create table` it holds and that the last migration
    /// leaves, with their last columns.
    tables: Vec<Table>,
    problems: Vec<Problem>,
}

#[derive(Serialize, Deserialize)]
struct Table {
    name: String,
    at: Span,
    columns: Vec<Column>,
}

#[derive(Serialize, Deserialize)]
struct Column {
    name: String,
    kind: String,
    flick: String,
    nullable: bool,
    write: String,
}

#[derive(Serialize, Deserialize)]
struct Problem {
    at: Span,
    message: String,
    unsupported: bool,
}

/// Whether `block` is a migration, not a table.
pub fn is_migration(block: &Block) -> bool {
    (block.arguments().first()).is_some_and(|t| block.text_of(t.span) == "migration")
}

/// The schema each module's migrations build, from the migrations of
/// `blocks` in order.
pub fn collect(blocks: &[Block]) -> State {
    let mut by_module: BTreeMap<&str, Vec<&Block>> = BTreeMap::new();
    for b in blocks.iter().filter(|b| is_migration(b)) {
        by_module.entry(&b.module).or_default().push(b);
    }
    let mut state = State::default();
    for (module, blocks) in by_module {
        let migrations: Vec<ddl::Migration> = (blocks.iter().enumerate())
            .map(|(i, b)| ddl::migration(b, i))
            .collect();
        let (tables, problems) = ddl::schema(&migrations);
        let mut steps: Vec<Step> = (blocks.iter().zip(&migrations))
            .map(|(b, m)| Step {
                text: b.text.clone(),
                version: m
                    .version
                    .map_or(0, |v| i32::try_from(v).unwrap_or(i32::MAX)),
                name: m.name.clone(),
                statements: m.statements.iter().map(ddl::Statement::sql).collect(),
                tables: Vec::new(),
                problems: Vec::new(),
            })
            .collect();
        for p in problems {
            let (i, at) = located(p.range);
            steps[i].problems.push(Problem {
                at,
                message: p.message,
                unsupported: p.unsupported,
            });
        }
        for t in tables {
            let (i, at) = located(t.at);
            let mut columns = Vec::new();
            for c in &t.columns {
                let words: Vec<&str> = c.ty.iter().map(String::as_str).collect();
                let known = dialect::column_type(&words)
                    .filter(|sql| sql.words.split(' ').count() == words.len());
                let Some(sql) = known else {
                    let (j, at) = located(c.range);
                    let known: Vec<&str> = dialect::TYPES.iter().map(|t| t.words).collect();
                    let message = format!(
                        "unknown SQL type `{}`; known: {}",
                        c.ty.join(" "),
                        known.join(", ")
                    );
                    steps[j].problems.push(Problem {
                        at,
                        message,
                        unsupported: true,
                    });
                    continue;
                };
                columns.push(Column {
                    name: c.name.clone(),
                    kind: sql.kind.to_owned(),
                    flick: sql.flick.to_owned(),
                    nullable: !c.not_null,
                    write: c.write().index().to_owned(),
                });
            }
            steps[i].tables.push(Table {
                name: t.name,
                at,
                columns,
            });
        }
        state.modules.insert(module.to_owned(), steps);
    }
    state
}

/// The migrations of the module of `block`, and the index of `block` there.
fn steps<'s>(block: &Block, state: &'s State) -> Option<(&'s [Step], usize)> {
    let steps = state.modules.get(&block.module)?;
    let i = steps.iter().position(|s| s.text == block.text)?;
    Some((steps, i))
}

/// The tables the migrations of each module leave, as queries read them.
pub fn shown(state: &State) -> Vec<Named> {
    let mut out = Vec::new();
    for (module, steps) in &state.modules {
        for t in steps.iter().flat_map(|s| &s.tables) {
            let columns = (t.columns.iter())
                .map(|c| ShownColumn {
                    name: c.name.clone(),
                    kind: c.kind.clone(),
                    base: c.flick.clone(),
                    ty: c.flick.clone(),
                    nullable: c.nullable,
                    write: c.write.clone(),
                    via: false,
                })
                .collect();
            let shown = Shown {
                sql: t.name.clone(),
                columns,
            };
            out.push((module.clone(), ddl::type_name(&t.name), shown));
        }
    }
    out
}

/// What does not apply in `block`.
pub fn check(block: &Block, state: &State) -> Vec<Diagnostic> {
    match steps(block, state) {
        Some((steps, i)) if !bare(steps, i) => problems(&steps[i]),
        _ => Vec::new(),
    }
}

/// Whether the `i`th migration declares nothing, which `arena` refuses
/// with what does not apply in it.
fn bare(steps: &[Step], i: usize) -> bool {
    i > 0 && steps[i].tables.is_empty()
}

fn problems(step: &Step) -> Vec<Diagnostic> {
    (step.problems.iter())
        .map(|p| {
            let code = if p.unsupported { MALFORMED } else { UNAPPLIED };
            Diagnostic::error(p.message.clone(), p.at).with_code(code)
        })
        .collect()
}

/// The code of `block`: the tables it creates and, in the first migration
/// of the module, the list of them all. Each `_` of it makes `check` report
/// what does not apply; a later migration that creates no table reports it
/// here, as it declares nothing.
pub fn arena(block: &Block, state: &State) -> Result<Arena, Vec<Diagnostic>> {
    let Some((steps, i)) = steps(block, state) else {
        let message = "the migration is no step of its module's schema";
        return Err(vec![Diagnostic::error(message, block.whole())]);
    };
    let step = &steps[i];
    if bare(steps, i) && !step.problems.is_empty() {
        return Err(problems(step));
    }
    let mut b = Builder::new(block.whole());
    let mut decls = Vec::new();
    if i == 0 {
        let word = block.arguments().first().map_or(block.whole(), |t| t.span);
        b.at(word);
        decls.extend(list(&mut b, steps));
    }
    for t in &step.tables {
        decls.extend(table(&mut b, t));
    }
    let group = b.group(decls);
    Ok(b.finish(group))
}

/// `migrations`, every migration of the module in the order written, for
/// `Database.Migrate.run`.
fn list(b: &mut Builder, steps: &[Step]) -> [NodeId; 2] {
    let named = |b: &mut Builder, n: &str| b.ty(Ty::Name(n.to_owned()));
    let list = named(b, "List");
    let migration = named(b, "Database.Migrate.Migration");
    let ty = b.ty(Ty::Apply {
        function: list,
        arguments: vec![migration],
    });
    let signature = b.decl(Decl::Signature {
        name: "migrations".into(),
        ty,
    });
    let all = (steps.iter())
        .map(|step| {
            let version = b.expr(Expr::Number(f64::from(step.version)));
            let name = b.expr(Expr::Text(step.name.clone()));
            let statements = (step.statements.iter())
                .map(|s| b.expr(Expr::Text(s.clone())))
                .collect();
            let statements = b.expr(Expr::Items(statements));
            b.expr(Expr::Record(vec![
                field("version", version),
                field("name", name),
                field("statements", statements),
            ]))
        })
        .collect();
    let all = b.expr(Expr::Items(all));
    let any = b.ty(Ty::Wildcard);
    let body = b.expr(Expr::Annot { expr: all, ty: any });
    let value = b.decl(Decl::Let {
        name: "migrations".into(),
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    });
    [signature, value]
}

fn field(label: &str, value: NodeId) -> Field {
    Field {
        label: label.to_owned(),
        value,
    }
}

/// The row type and the companion of a table a migration creates, at its
/// name in the `create table`.
fn table(b: &mut Builder, t: &Table) -> Vec<NodeId> {
    b.at(t.at);
    let named = |b: &mut Builder, n: &str| b.ty(Ty::Name(n.to_owned()));
    let base = |c: &Column| {
        if c.flick == "BigDecimal" {
            "BigDecimal.BigDecimal".to_owned()
        } else {
            c.flick.clone()
        }
    };
    let row: Vec<Field> = (t.columns.iter())
        .map(|c| {
            let ty = named(b, &base(c));
            let value = if c.nullable {
                let option = named(b, "Option");
                b.ty(Ty::Apply {
                    function: option,
                    arguments: vec![ty],
                })
            } else {
                ty
            };
            field(&c.name, value)
        })
        .collect();
    let shapes: Vec<Field> = (t.columns.iter())
        .map(|c| {
            let column = named(b, "Sql.Column");
            let ty = named(b, &base(c));
            let null = named(
                b,
                if c.nullable {
                    "Sql.Nullable"
                } else {
                    "Sql.NonNull"
                },
            );
            let write = named(b, &format!("Sql.{}", c.write));
            let shape = b.ty(Ty::Apply {
                function: column,
                arguments: vec![ty, null, write],
            });
            field(&c.name, shape)
        })
        .collect();
    let runtime: Vec<NodeId> = (t.columns.iter())
        .map(|c| {
            let plain = b.expr(Expr::Name("Sql.Derive.plain".into()));
            let name = b.expr(Expr::Text(c.name.clone()));
            let kind = b.expr(Expr::Text(c.kind.clone()));
            let nullable = b.expr(Expr::Boolean(c.nullable));
            let same = b.expr(Expr::Name("Sql.Derive.same".into()));
            b.expr(Expr::Apply {
                function: plain,
                arguments: vec![name, kind, nullable, same],
            })
        })
        .collect();
    let row_name = ddl::type_name(&t.name);
    let row = b.ty(Ty::Record(row));
    let alias = b.decl(Decl::Alias {
        name: row_name.clone(),
        params: Vec::new(),
        ty: row,
    });
    let companion = format!("{row_name}.table");
    let sql_table = named(b, "Sql.Table");
    let sql_name = b.ty(Ty::Text(t.name.clone()));
    let shapes = b.ty(Ty::Record(shapes));
    let ty = b.ty(Ty::Apply {
        function: sql_table,
        arguments: vec![sql_name, shapes],
    });
    let signature = b.decl(Decl::Signature {
        name: companion.clone(),
        ty,
    });
    let name = b.expr(Expr::Text(t.name.clone()));
    let columns = b.expr(Expr::Items(runtime));
    let record = b.expr(Expr::Record(vec![
        field("name", name),
        field("columns", columns),
    ]));
    let any = b.ty(Ty::Wildcard);
    let record = b.expr(Expr::Annot {
        expr: record,
        ty: any,
    });
    let make = b.expr(Expr::Name("Sql.make_table".into()));
    let body = b.expr(Expr::Apply {
        function: make,
        arguments: vec![record],
    });
    let value = b.decl(Decl::Let {
        name: companion,
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    });
    vec![alias, signature, value]
}
