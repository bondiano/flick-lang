//! `sql table` declarations and `sql { … }` blocks: plain data for the
//! runtime, `Sql.make_table` and `Sql.make_template` applied to records of
//! the names, kinds and codecs the checker settled.

use super::Lower;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::{SqlForm, SqlValue, companion_name};

impl Lower<'_> {
    /// `Name$table`: the table's SQL name and its columns.
    pub(super) fn sql_table(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        let Some(table) = self
            .typing
            .sql_tables
            .iter()
            .find(|t| t.decl == decl.range())
        else {
            return;
        };
        self.fresh_definition();
        let columns = table
            .columns
            .iter()
            .map(|c| {
                let codec = match c.via.and_then(|r| find(decl, r)) {
                    Some(node) => self.expr(node),
                    None => lit(Lit::Unit, span),
                };
                record(
                    vec![
                        ("name", string(&c.name, span)),
                        ("kind", string(&c.kind, span)),
                        ("nullable", lit(Lit::Bool(c.nullable), span)),
                        ("codec", codec),
                    ],
                    span,
                )
            })
            .collect();
        let body = record(
            vec![
                ("name", string(&table.table, span)),
                ("columns", Expr::new(ExprKind::List(columns), span)),
            ],
            span,
        );
        items.push(Item {
            kind: ItemKind::Value {
                name: companion_name(&table.name, "table"),
                dicts: Vec::new(),
                body: call("make_table", body, span),
            },
            span,
        });
    }

    /// A checked query: its text, tables, parameters and result columns.
    pub(super) fn sql_block(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let Some(plan) = self.typing.sql.get(&node.range()) else {
            return lit(Lit::Unit, span);
        };
        let tables = plan
            .tables
            .iter()
            .filter_map(|r| find(node, *r))
            .map(|t| self.expr(t))
            .collect();
        let params = plan
            .params
            .iter()
            .map(|p| self.sql_value(node, p, span))
            .collect();
        let columns = plan
            .columns
            .iter()
            .map(|c| self.sql_value(node, c, span))
            .collect();
        let body = record(
            vec![
                ("text", string(&plan.text, span)),
                ("tables", Expr::new(ExprKind::List(tables), span)),
                ("params", Expr::new(ExprKind::List(params), span)),
                ("columns", Expr::new(ExprKind::List(columns), span)),
            ],
            span,
        );
        call("make_template", body, span)
    }

    fn sql_value(&mut self, block: &Node, v: &SqlValue, span: Span) -> Expr {
        let mut fields = vec![
            ("name", string(&v.name, span)),
            ("nullable", lit(Lit::Bool(v.nullable), span)),
        ];
        match &v.form {
            SqlForm::Kind(kind) => fields.push(("kind", string(kind, span))),
            SqlForm::Column(table, column) => {
                fields.push(("table", lit(Lit::Number(table.to_string()), span)));
                fields.push(("column", string(column, span)));
            }
        }
        if let Some(node) = v.literal.and_then(|r| find(block, r)) {
            fields.push(("value", self.expr(node)));
        }
        record(fields, span)
    }
}

/// The node of `range` inside `root`.
fn find(root: &Node, range: (u32, u32)) -> Option<&Node> {
    if root.range() == range {
        return Some(root);
    }
    root.nodes()
        .filter(|n| n.range().0 <= range.0 && range.1 <= n.range().1)
        .find_map(|n| find(n, range))
}

fn lit(l: Lit, span: Span) -> Expr {
    Expr::new(ExprKind::Lit(l), span)
}

fn string(s: &str, span: Span) -> Expr {
    lit(Lit::String(s.to_owned()), span)
}

fn record(fields: Vec<(&str, Expr)>, span: Span) -> Expr {
    let fields = fields.into_iter().map(|(l, e)| (l.to_owned(), e)).collect();
    Expr::new(ExprKind::Record(fields), span)
}

fn call(name: &str, arg: Expr, span: Span) -> Expr {
    let func = Expr::new(
        ExprKind::Global {
            module: "Sql".into(),
            name: name.into(),
        },
        span,
    );
    Expr::new(
        ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            runtime: false,
            arg_first: false,
        },
        span,
    )
}
