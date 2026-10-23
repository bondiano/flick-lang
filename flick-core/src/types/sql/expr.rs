//! SQL expressions inside `sql { … }`: columns of the query's aliases,
//! parameters, literals as placeholders, comparisons in SQL's three-valued
//! logic, `is_null`, `is_not_null`, null-safe `is_not_distinct_from`,
//! `coalesce`, aggregates with their phase, `exists` and `is_in` over
//! uncorrelated subqueries.

use super::super::Checker;
use super::super::infer::literal_type;
use super::super::ty::Type;
use super::super::typing::{SqlForm, range};
use super::query::{Alias, Cx, Scope};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// Where an expression stands: aggregates belong to `select`, `having`
/// and `order_by` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Row,
    Grouped,
}

/// An elaborated SQL expression.
#[derive(Debug, Clone)]
pub struct Val {
    pub sql: String,
    pub ty: Type,
    /// Whether it may be `NULL`.
    pub null: bool,
    /// Whether it contains an aggregate.
    pub agg: bool,
    /// Columns used outside any aggregate, as `(alias, column)`.
    pub bare: Vec<(String, String)>,
    /// How its value crosses the driver, when a column decides it.
    pub form: Option<SqlForm>,
    /// The parameter it is, by index into the query's parameters.
    pub param: Option<usize>,
}

impl Val {
    fn plain(sql: String, ty: Type) -> Val {
        Val {
            sql,
            ty,
            null: false,
            agg: false,
            bare: Vec::new(),
            form: None,
            param: None,
        }
    }

    fn error() -> Val {
        Val::plain(String::new(), Type::Error)
    }
}

/// `"name"` with inner quotes doubled.
pub fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// The expression inside one-statement blocks and parentheses-free wrappers.
pub fn inner(node: &Node) -> &Node {
    let mut node = node;
    while node.kind == K::Block {
        match node.nodes().collect::<Vec<_>>()[..] {
            [only] => node = only,
            _ => break,
        }
    }
    node
}

const FUNCTIONS: &str = "is_null, is_not_null, is_not_distinct_from, coalesce, count, \
                         count_all, sum, avg, min, max, exists, is_in";

impl Checker<'_> {
    pub(super) fn sql_value(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        node: &Node,
        place: Place,
    ) -> Val {
        let node = inner(node);
        let children: Vec<&Node> = node.nodes().collect();
        let value = match node.kind {
            K::ParenExpr if children.len() == 1 => {
                let mut v = self.sql_value(cx, scope, children[0], place);
                v.sql = format!("({})", v.sql);
                v
            }
            K::Literal => self.sql_literal(cx, node),
            K::NameExpr => self.sql_name(scope, node, place),
            K::FieldExpr => self.sql_field(cx, scope, node, children[0]),
            K::BinExpr if children.len() == 2 => {
                self.sql_binary(cx, scope, node, children[0], children[1], place)
            }
            K::PrefixExpr if children.len() == 1 => {
                let mut v = self.sql_value(cx, scope, children[0], place);
                if node.has_token(K::NotKw) {
                    self.expect(&v.ty, &Type::prim("Bool"), self.span(children[0]));
                    v.sql = format!("(NOT {})", v.sql);
                } else {
                    self.sql_numeric(&v, children[0], "-");
                    v.sql = format!("(-{})", v.sql);
                    v.form = None;
                }
                v.param = None;
                v
            }
            K::AppExpr => self.sql_call(cx, scope, node, &children, place),
            K::SqlExpr => {
                self.error(
                    codes::Q010,
                    self.span(node),
                    "a subquery goes in `exists`, `is_in` or `union`",
                );
                Val::error()
            }
            _ => {
                self.error(
                    codes::Q010,
                    self.span(node),
                    "this is not a SQL expression: use columns, parameters, literals, operators and SQL functions",
                );
                Val::error()
            }
        };
        self.note_type(
            self.span(node),
            &super::query::optional(value.ty.clone(), value.null),
        );
        value
    }

    /// A literal: `true` and `false` inline, anything else a placeholder.
    fn sql_literal(&mut self, cx: &mut Cx, node: &Node) -> Val {
        let token = node.child_tokens().next().map(|t| t.kind);
        let ty = literal_type(token);
        match token {
            Some(K::TrueKw) => Val::plain("TRUE".into(), ty),
            Some(K::FalseKw) => Val::plain("FALSE".into(), ty),
            _ => {
                let at = cx.literal(range(self.span(node)), ty.clone());
                Val::plain(format!("${}", at + 1), ty)
            }
        }
    }

    /// `count_all`, an unqualified column, or `Table.column` in a statement
    /// on one table.
    fn sql_name(&mut self, scope: &Scope, node: &Node, place: Place) -> Val {
        let parts: Vec<&str> = node
            .child_tokens()
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(self.text))
            .collect();
        let span = self.span(node);
        match parts[..] {
            ["count_all"] => {
                self.sql_aggregate_allowed(place, node);
                let mut v = Val::plain("COUNT(*)".into(), Type::prim("BigInt"));
                v.agg = true;
                v
            }
            [name] if name.starts_with(|c: char| c.is_ascii_lowercase()) => {
                let found: Vec<_> = scope
                    .aliases
                    .iter()
                    .filter(|a| a.columns.iter().any(|c| c.name == name))
                    .collect();
                match found[..] {
                    [alias] => column_of(alias, name),
                    [] => {
                        self.error(
                            codes::Q001,
                            span,
                            format!("no table of this query has a column `{name}`"),
                        );
                        Val::error()
                    }
                    _ => {
                        let names: Vec<String> =
                            found.iter().map(|a| format!("`{}`", a.name)).collect();
                        self.error(
                            codes::Q003,
                            span,
                            format!(
                                "`{name}` is a column of {}: name the alias",
                                names.join(" and ")
                            ),
                        );
                        Val::error()
                    }
                }
            }
            [alias, column] => self.sql_qualified(scope, alias, column, span),
            _ => {
                self.error(codes::Q010, span, "this is not a column of the query");
                Val::error()
            }
        }
    }

    /// `alias.column` or `params.name`.
    fn sql_field(&mut self, cx: &mut Cx, scope: &Scope, node: &Node, target: &Node) -> Val {
        let span = self.span(node);
        let Some(label) = node.child_tokens().find(|t| t.kind == K::Ident) else {
            return Val::error();
        };
        let label = label.text(self.text);
        let name = (target.kind == K::NameExpr)
            .then(|| target.child_tokens().collect::<Vec<_>>())
            .filter(|t| t.len() == 1 && t[0].kind == K::Ident)
            .map(|t| t[0].text(self.text));
        let Some(name) = name else {
            self.error(codes::Q010, span, "this is not a column of the query");
            return Val::error();
        };
        if cx.binder.as_deref() == Some(name) {
            let at = cx.param(label, span, || self.store.fresh());
            let p = &cx.params[at];
            return Val {
                sql: format!("${}", p.placeholder + 1),
                ty: p.ty.clone(),
                null: p.nullable,
                agg: false,
                bare: Vec::new(),
                form: p.form.clone(),
                param: Some(at),
            };
        }
        self.sql_qualified(scope, name, label, span)
    }

    /// `alias.column` of an alias of the query.
    fn sql_qualified(&mut self, scope: &Scope, alias: &str, column: &str, span: Span) -> Val {
        let Some(a) = scope.aliases.iter().find(|a| a.name == alias) else {
            self.error(
                codes::Q002,
                span,
                format!("no table `{alias}` in this query"),
            );
            return Val::error();
        };
        if !a.columns.iter().any(|c| c.name == column) {
            let message = format!("`{}` has no column `{column}`", a.table_name);
            self.error(codes::Q001, span, message);
            return Val::error();
        }
        column_of(a, column)
    }

    fn sql_binary(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        node: &Node,
        lhs: &Node,
        rhs: &Node,
        place: Place,
    ) -> Val {
        let Some(op) = node.child_tokens().find(|t| t.kind.is_binary_op()) else {
            return Val::error();
        };
        let l = self.sql_value(cx, scope, lhs, place);
        let r = self.sql_value(cx, scope, rhs, place);
        let bool_ = Type::prim("Bool");
        let (sql_op, result) = match op.kind {
            K::AndAnd | K::OrOr => {
                self.expect(&l.ty, &bool_, self.span(lhs));
                self.expect(&r.ty, &bool_, self.span(rhs));
                (if op.kind == K::AndAnd { "AND" } else { "OR" }, bool_)
            }
            K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge => {
                self.expect(&r.ty, &l.ty, self.span(rhs));
                cx.adopt(&l, &r);
                let sql_op = match op.kind {
                    K::EqEq => "=",
                    K::NotEq => "<>",
                    K::Lt => "<",
                    K::Le => "<=",
                    K::Gt => ">",
                    _ => ">=",
                };
                (sql_op, bool_)
            }
            K::PlusPlus => {
                self.expect(&l.ty, &Type::prim("String"), self.span(lhs));
                self.expect(&r.ty, &Type::prim("String"), self.span(rhs));
                ("||", Type::prim("String"))
            }
            K::Plus | K::Minus | K::Star | K::Slash | K::Percent => {
                self.expect(&r.ty, &l.ty, self.span(rhs));
                cx.adopt(&l, &r);
                self.sql_numeric(&l, lhs, op.text(self.text));
                (op.text(self.text), l.ty.clone())
            }
            _ => {
                self.error(
                    codes::Q010,
                    self.token_span(op),
                    format!("`{}` has no SQL form", op.text(self.text)),
                );
                return Val::error();
            }
        };
        let mut bare = l.bare;
        bare.extend(r.bare);
        Val {
            sql: format!("({} {sql_op} {})", l.sql, r.sql),
            ty: result,
            null: l.null || r.null,
            agg: l.agg || r.agg,
            bare,
            form: None,
            param: None,
        }
    }

    /// Arithmetic needs `Number`, `BigInt` or `BigDecimal`.
    fn sql_numeric(&mut self, v: &Val, node: &Node, op: &str) {
        let ty = self.whnf(&self.zonk(&v.ty));
        if !matches!(ty, Type::Meta(_) | Type::Error) && numeric(&ty).is_none() {
            let shown = self.show(&ty);
            self.error(
                codes::Q010,
                self.span(node),
                format!("`{op}` in SQL takes `Number`, `BigInt` or `BigDecimal`, not `{shown}`"),
            );
        }
    }

    fn sql_aggregate_allowed(&mut self, place: Place, node: &Node) {
        if place == Place::Row {
            self.error(
                codes::Q006,
                self.span(node),
                "an aggregate is computed per group: use it in `select`, `having` or `order_by`",
            );
        }
    }

    fn sql_call(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        node: &Node,
        children: &[&Node],
        place: Place,
    ) -> Val {
        let head = children[0];
        let args = &children[1..];
        let name = (head.kind == K::NameExpr)
            .then(|| head.child_tokens().collect::<Vec<_>>())
            .filter(|t| t.len() == 1 && t[0].kind == K::Ident)
            .map(|t| t[0].text(self.text).to_owned())
            .unwrap_or_default();
        let span = self.span(node);
        let arity = match name.as_str() {
            "is_null" | "is_not_null" | "count" | "sum" | "avg" | "min" | "max" | "exists" => 1,
            "is_not_distinct_from" | "coalesce" | "is_in" => 2,
            _ => {
                self.error(
                    codes::Q010,
                    self.span(head),
                    format!(
                        "`{}` is not a SQL function; known: {FUNCTIONS}",
                        self.text_of(head)
                    ),
                );
                return Val::error();
            }
        };
        if args.len() != arity {
            self.error(
                codes::Q010,
                span,
                format!(
                    "`{name}` takes {arity} argument{}",
                    if arity == 1 { "" } else { "s" }
                ),
            );
            return Val::error();
        }
        match name.as_str() {
            "exists" | "is_in" => self.sql_membership(cx, scope, &name, args, place),
            "is_null" | "is_not_null" => {
                let v = self.sql_value(cx, scope, args[0], place);
                let test = if name == "is_null" {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
                Val {
                    sql: format!("({} {test})", v.sql),
                    ty: Type::prim("Bool"),
                    null: false,
                    form: None,
                    param: None,
                    ..v
                }
            }
            "is_not_distinct_from" | "coalesce" => {
                let a = self.sql_value(cx, scope, args[0], place);
                let b = self.sql_value(cx, scope, args[1], place);
                self.expect(&b.ty, &a.ty, self.span(args[1]));
                cx.adopt(&a, &b);
                let mut bare = a.bare.clone();
                bare.extend(b.bare.clone());
                if name == "coalesce" {
                    Val {
                        sql: format!("COALESCE({}, {})", a.sql, b.sql),
                        null: a.null && b.null,
                        agg: a.agg || b.agg,
                        bare,
                        form: a.form.clone().or(b.form.clone()),
                        param: None,
                        ty: a.ty,
                    }
                } else {
                    Val {
                        sql: format!("({} IS NOT DISTINCT FROM {})", a.sql, b.sql),
                        ty: Type::prim("Bool"),
                        null: false,
                        agg: a.agg || b.agg,
                        bare,
                        form: None,
                        param: None,
                    }
                }
            }
            _ => self.sql_aggregate(cx, scope, node, &name, args[0], place),
        }
    }

    /// `exists q` and `is_in e q` over an uncorrelated subquery.
    fn sql_membership(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        name: &str,
        args: &[&Node],
        place: Place,
    ) -> Val {
        if name == "exists" {
            let Some(sub) = self.sql_subquery(cx, args[0]) else {
                return Val::error();
            };
            return Val::plain(format!("EXISTS ({})", sub.sql), Type::prim("Bool"));
        }
        let v = self.sql_value(cx, scope, args[0], place);
        let Some(sub) = self.sql_subquery(cx, args[1]) else {
            return Val::error();
        };
        if let [only] = &sub.columns[..] {
            self.expect(&only.ty, &v.ty, self.span(args[1]));
        } else {
            self.error(
                codes::Q010,
                self.span(args[1]),
                "the subquery of `is_in` selects exactly one result",
            );
        }
        let null = v.null || sub.columns.iter().any(|c| c.null);
        Val {
            sql: format!("({} IN ({}))", v.sql, sub.sql),
            null,
            ty: Type::prim("Bool"),
            form: None,
            param: None,
            ..v
        }
    }

    /// `count`, `sum`, `avg`, `min`, `max`: `NULL` over no rows except
    /// `count`; sums and averages cast to the type their result has.
    fn sql_aggregate(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        node: &Node,
        name: &str,
        arg: &Node,
        place: Place,
    ) -> Val {
        self.sql_aggregate_allowed(place, node);
        let v = self.sql_value(cx, scope, arg, Place::Row);
        let numeric_ty = numeric(&self.whnf(&self.zonk(&v.ty)));
        let (sql, ty, null, form) = match name {
            "count" => (
                format!("COUNT({})", v.sql),
                Type::prim("BigInt"),
                false,
                None,
            ),
            "min" | "max" => (
                format!("{}({})", name.to_uppercase(), v.sql),
                v.ty.clone(),
                true,
                v.form.clone(),
            ),
            _ => {
                let Some(n) = numeric_ty else {
                    let shown = self.show(&v.ty);
                    self.error(
                        codes::Q010,
                        self.span(arg),
                        format!("`{name}` takes `Number`, `BigInt` or `BigDecimal`, not `{shown}`"),
                    );
                    return Val::error();
                };
                let upper = name.to_uppercase();
                let (sql, ty) = match (name, n) {
                    (_, "number") => (
                        format!("{upper}({})::double precision", v.sql),
                        v.ty.clone(),
                    ),
                    ("sum", "bigint") => (format!("SUM({})::bigint", v.sql), v.ty.clone()),
                    _ => (
                        format!("{upper}({})::numeric", v.sql),
                        Type::con("BigDecimal", "BigDecimal"),
                    ),
                };
                (sql, ty, true, None)
            }
        };
        Val {
            sql,
            ty,
            null,
            agg: true,
            bare: Vec::new(),
            form,
            param: None,
        }
    }
}

/// A column of an alias that has it.
pub fn column_of(alias: &Alias, column: &str) -> Val {
    let col = alias
        .columns
        .iter()
        .find(|c| c.name == column)
        .expect("the caller found the column");
    Val {
        sql: format!("{}.{}", quote(&alias.name), quote(column)),
        ty: col.ty.clone(),
        null: col.nullable || alias.nullable,
        agg: false,
        bare: vec![(alias.name.clone(), column.to_owned())],
        form: Some(SqlForm::Column(alias.table, column.to_owned())),
        param: None,
    }
}

/// The runtime kind of a numeric type.
pub fn numeric(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Con { module, name } => match (module.as_str(), name.as_str()) {
            ("Prelude", "Number") => Some("number"),
            ("Prelude", "BigInt") => Some("bigint"),
            ("BigDecimal", "BigDecimal") => Some("numeric"),
            _ => None,
        },
        _ => None,
    }
}
