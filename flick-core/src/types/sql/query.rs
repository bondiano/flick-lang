//! `sql [params] { clauses }`: a query or statement over aliases of tables,
//! checked clause by clause and turned into a `Sql.Template Pg p r` whose
//! parameters `p` and rows `r` follow from the expressions. A `LEFT JOIN`
//! makes its alias nullable, so its columns read as `Option` once, however
//! many joins make them nullable.

use super::super::Checker;
use super::super::ty::{Row, Type};
use super::super::typing::{SqlForm, SqlPlan, SqlValue, range};
use super::expr::{Place, Val, inner, quote};
use super::table::{SQL, index};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// A table in the scope of a query.
#[derive(Debug, Clone)]
pub struct Alias {
    pub name: String,
    /// How diagnostics name the table.
    pub table_name: String,
    /// The table's SQL name.
    pub table_sql: String,
    /// Index of the table's expression in the plan.
    pub table: usize,
    pub columns: Vec<Col>,
    /// Joined with `left_join`: every column may be `NULL`.
    pub nullable: bool,
}

impl Alias {
    pub fn row_type(&self) -> Type {
        Type::Record(Row::closed(
            self.columns
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        optional(c.ty.clone(), self.nullable || c.nullable),
                    )
                })
                .collect(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct Col {
    pub name: String,
    pub ty: Type,
    pub nullable: bool,
    pub write: String,
}

/// The aliases a query sees.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub aliases: Vec<Alias>,
}

/// A named parameter of the query.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub nullable: bool,
    pub form: Option<SqlForm>,
    pub span: Span,
    pub placeholder: usize,
}

/// What a query and its subqueries share: the parameter binder, the
/// parameters and placeholders, the tables.
pub struct Cx {
    pub binder: Option<String>,
    pub params: Vec<Param>,
    pub plan: SqlPlan,
    /// The type of each placeholder, in order.
    pub slots: Vec<Type>,
}

impl Cx {
    /// The named parameter `name`, added at its first use.
    pub fn param(&mut self, name: &str, span: Span, fresh: impl FnOnce() -> Type) -> usize {
        if let Some(i) = self.params.iter().position(|p| p.name == name) {
            return i;
        }
        let ty = fresh();
        self.params.push(Param {
            name: name.to_owned(),
            ty: ty.clone(),
            nullable: false,
            form: None,
            span,
            placeholder: self.slots.len(),
        });
        self.slots.push(ty);
        self.plan.params.push(SqlValue {
            name: name.to_owned(),
            literal: None,
            form: SqlForm::Kind(String::new()),
            nullable: false,
        });
        self.params.len() - 1
    }

    /// A placeholder for a literal; returns its index.
    pub fn literal(&mut self, at: (u32, u32), ty: Type) -> usize {
        self.slots.push(ty);
        self.plan.params.push(SqlValue {
            name: String::new(),
            literal: Some(at),
            form: SqlForm::Kind(String::new()),
            nullable: false,
        });
        self.slots.len() - 1
    }

    /// A parameter compared with a column passes through the column's codec.
    pub fn adopt(&mut self, a: &Val, b: &Val) {
        for (p, other) in [(a, b), (b, a)] {
            if let (Some(i), Some(form)) = (p.param, &other.form)
                && self.params[i].form.is_none()
            {
                self.params[i].form = Some(form.clone());
            }
        }
    }
}

/// A result column of a query.
#[derive(Debug, Clone)]
pub struct Out {
    pub label: String,
    pub ty: Type,
    pub null: bool,
    pub form: Option<SqlForm>,
}

/// A checked query: its SQL and its results.
pub struct Query {
    pub sql: String,
    pub columns: Vec<Out>,
}

/// The clauses of a block: each word with its node.
fn clauses<'n>(node: &'n Node, text: &str) -> Vec<(String, &'n Node)> {
    node.nodes()
        .filter(|n| n.kind == K::Block)
        .flat_map(Node::nodes)
        .filter(|n| n.kind == K::SqlClause)
        .filter_map(|c| Some((c.child_tokens().next()?.text(text).to_owned(), c)))
        .collect()
}

impl Checker<'_> {
    /// `sql [params] { … }`: a `Sql.Template Pg p r`.
    pub fn sql_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let binder = node.child_tokens().find(|t| t.kind == K::Ident);
        let declared = expected.and_then(|e| self.sql_declared_params(e));
        if let (Some(token), Some(row)) = (binder, &declared) {
            self.note_type(
                Span::new(self.file, token.start, token.end),
                &Type::Record(row.clone()),
            );
        }
        let mut cx = Cx {
            binder: binder.map(|t| t.text(self.text).to_owned()),
            params: Vec::new(),
            plan: SqlPlan::default(),
            slots: Vec::new(),
        };
        let Some(query) = self.sql_statement(&mut cx, node) else {
            return Type::Error;
        };
        if let Some(declared) = &declared {
            let unknown: Vec<(String, Span)> = cx
                .params
                .iter()
                .filter(|p| declared.get(&p.name).is_none())
                .map(|p| (p.name.clone(), p.span))
                .collect();
            for (name, span) in &unknown {
                self.error(
                    codes::Q009,
                    *span,
                    format!("the template declares no parameter `{name}`"),
                );
            }
            if !unknown.is_empty() {
                return Type::Error;
            }
        }
        let params = Row::closed(
            cx.params
                .iter()
                .map(|p| (p.name.clone(), optional(p.ty.clone(), p.nullable)))
                .collect(),
        );
        if let Some(token) = binder {
            self.note_type(
                Span::new(self.file, token.start, token.end),
                &Type::Record(declared.as_ref().unwrap_or(&params).clone()),
            );
        }
        let rows = Row::closed(
            query
                .columns
                .iter()
                .map(|c| (c.label.clone(), optional(c.ty.clone(), c.null)))
                .collect(),
        );
        let ty = Type::apps(
            Type::con(SQL, "Template"),
            [index("Pg"), Type::Record(params), Type::Record(rows)],
        );
        if let Some(expected) = expected {
            self.expect(&ty, expected, self.span(node));
        }
        self.sql_plan(cx, query, node);
        ty
    }

    /// The parameters a `Template` type declares, when it is one.
    fn sql_declared_params(&mut self, expected: &Type) -> Option<Row> {
        let ty = self.whnf(&self.zonk(expected));
        let (head, args) = ty.spine();
        let is_template =
            matches!(head, Type::Con { module, name } if module == SQL && name == "Template");
        if !is_template || args.len() != 3 {
            return None;
        }
        match self.whnf(args[1]) {
            Type::Record(row) if row.tail.is_none() => Some(row),
            _ => None,
        }
    }

    /// Settles how each parameter and result crosses the driver.
    fn sql_plan(&mut self, mut cx: Cx, query: Query, node: &Node) {
        for p in &cx.params {
            let form = self.sql_form(p.form.clone(), &p.ty, p.span);
            let slot = &mut cx.plan.params[p.placeholder];
            slot.form = form;
            slot.nullable = p.nullable;
        }
        for (i, ty) in cx.slots.iter().enumerate() {
            if cx.plan.params[i].literal.is_some() {
                let span = self.span(node);
                cx.plan.params[i].form = self.sql_form(None, ty, span);
            }
        }
        cx.plan.columns = query
            .columns
            .iter()
            .map(|c| SqlValue {
                name: c.label.clone(),
                literal: None,
                form: self.sql_form(c.form.clone(), &c.ty, self.span(node)),
                nullable: c.null,
            })
            .collect();
        cx.plan.text = query.sql;
        self.typing.sql.insert(range(self.span(node)), cx.plan);
    }

    /// A column's form, or the kind of a SQL base type.
    fn sql_form(&mut self, form: Option<SqlForm>, ty: &Type, span: Span) -> SqlForm {
        if let Some(form) = form {
            return form;
        }
        let ty = self.whnf(&self.zonk(ty));
        let kind = super::expr::numeric(&ty).or(match &ty {
            Type::Con { module, name } if module == "Prelude" => match name.as_str() {
                "String" => Some("text"),
                "Bool" => Some("bool"),
                "Json" => Some("json"),
                _ => None,
            },
            _ => None,
        });
        if let Some(kind) = kind {
            return SqlForm::Kind(kind.to_owned());
        }
        if ty != Type::Error {
            let shown = self.show(&ty);
            self.error(
                codes::Q012,
                span,
                format!("`{shown}` has no SQL form: compare it with a column or give the template's type"),
            );
        }
        SqlForm::Kind(String::new())
    }

    /// A whole query or statement block, nested or not.
    pub(super) fn sql_statement(&mut self, cx: &mut Cx, node: &Node) -> Option<Query> {
        let clauses = clauses(node, self.text);
        let Some((first, _)) = clauses.first() else {
            self.error(codes::Q010, self.span(node), "an empty `sql` block");
            return None;
        };
        match first.as_str() {
            "from" => self.sql_select(cx, &clauses),
            "insert_into" | "update" | "delete_from" => self.sql_write(cx, &clauses),
            other => {
                self.error(
                    codes::Q010,
                    self.span(clauses[0].1),
                    format!("a query starts with `from`, `insert_into`, `update` or `delete_from`, not `{other}`"),
                );
                None
            }
        }
    }

    /// A subquery: its own scope, the parameters of the enclosing query.
    pub(super) fn sql_subquery(&mut self, cx: &mut Cx, node: &Node) -> Option<Query> {
        let node = match inner(node) {
            n if n.kind == K::ParenExpr => n.nodes().next()?,
            n => n,
        };
        if node.kind != K::SqlExpr {
            self.error(
                codes::Q010,
                self.span(node),
                "expected a subquery `sql { … }`",
            );
            return None;
        }
        if node.child_tokens().any(|t| t.kind == K::Ident) {
            self.error(
                codes::Q010,
                self.span(node),
                "a subquery uses the parameters of its query and names none of its own",
            );
        }
        self.sql_statement(cx, node)
    }

    /// The table of `from x in T`: a `sql table` by name, or any
    /// expression of type `Sql.Table`.
    pub(super) fn sql_table_alias(
        &mut self,
        cx: &mut Cx,
        alias: &str,
        table: &Node,
    ) -> Option<Alias> {
        let span = self.span(table);
        let tokens: Vec<_> = table.child_tokens().filter(|t| t.kind != K::Dot).collect();
        let named = table.kind == K::NameExpr && tokens.last().is_some_and(|t| t.kind == K::Con);
        let (ty, table_name) = if named {
            let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
            let (name, qualifier) = parts.split_last()?;
            let (module, _) = self.find_type(table, &qualifier.join("."), name)?;
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Value,
                name: super::super::companion_name(name, "table"),
            };
            let Some(scheme) = self.env.value(&entity).cloned() else {
                self.error(codes::Q010, span, format!("`{name}` is not a `sql table`"));
                return None;
            };
            let ty = self.instantiate(&scheme, span);
            self.record_entity(&entity, &ty, span);
            (ty, (*name).to_owned())
        } else {
            (self.infer(table), self.text_of(table))
        };
        let (table_sql, columns) = self.sql_columns(&ty, span)?;
        cx.plan.tables.push(range(span));
        Some(Alias {
            name: alias.to_owned(),
            table_name,
            table_sql,
            table: cx.plan.tables.len() - 1,
            columns,
            nullable: false,
        })
    }

    /// The SQL name and the columns of a
    /// `Sql.Table "name" { c : Sql.Column t n w, … }`.
    fn sql_columns(&mut self, ty: &Type, span: Span) -> Option<(String, Vec<Col>)> {
        let ty = self.whnf(&self.zonk(ty));
        let (head, args) = ty.spine();
        let table = match (head, &args[..]) {
            (Type::Con { module, name }, [Type::Symbol(table), cols])
                if module == SQL && name == "Table" =>
            {
                match self.whnf(cols) {
                    Type::Record(row) => Some((table.clone(), row)),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some((table, row)) = table else {
            if ty != Type::Error {
                let shown = self.show(&ty);
                self.error(
                    codes::T003,
                    span,
                    format!("expected a `Sql.Table`, found `{shown}`"),
                );
            }
            return None;
        };
        let con_name = |t: &Type| match t {
            Type::Con { name, .. } => name.clone(),
            _ => String::new(),
        };
        Some((
            table,
            row.entries
                .iter()
                .filter_map(|(label, col)| match col.spine().1[..] {
                    [t, n, w] => Some(Col {
                        name: label.clone(),
                        ty: t.clone(),
                        nullable: con_name(n) == "Nullable",
                        write: con_name(w),
                    }),
                    _ => None,
                })
                .collect(),
        ))
    }

    /// `select { label = e; … }`, `select a.x, b.y` or `select alias`; the
    /// same forms after `returning`.
    pub(super) fn sql_projection(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        clause: &Node,
        place: Place,
    ) -> Vec<(Out, String, Val, Span)> {
        let mut items: Vec<(String, Val, Span)> = Vec::new();
        let exprs: Vec<&Node> = clause.nodes().collect();
        for e in &exprs {
            let e = inner(e);
            if e.kind == K::RecordExpr {
                for field in e.nodes().filter(|n| n.kind == K::RecordField) {
                    let label = field.child_tokens().find(|t| t.kind == K::Ident);
                    let (Some(label), Some(value)) = (label, field.nodes().next()) else {
                        continue;
                    };
                    let v = self.sql_value(cx, scope, value, place);
                    self.note_type(
                        Span::new(self.file, label.start, label.end),
                        &optional(v.ty.clone(), v.null),
                    );
                    items.push((label.text(self.text).to_owned(), v, self.span(field)));
                }
                continue;
            }
            let lone = (e.kind == K::NameExpr)
                .then(|| {
                    e.child_tokens()
                        .map(|t| t.text(self.text))
                        .collect::<Vec<_>>()
                })
                .filter(|t| t.len() == 1);
            if let Some(alias) = lone.and_then(|t| scope.aliases.iter().find(|a| a.name == t[0])) {
                self.note_type(self.span(e), &alias.row_type());
                for col in &alias.columns.clone() {
                    let v = super::expr::column_of(alias, &col.name);
                    items.push((col.name.clone(), v, self.span(e)));
                }
                continue;
            }
            let label = match e.kind {
                K::FieldExpr => e.child_tokens().find(|t| t.kind == K::Ident),
                K::NameExpr => e.child_tokens().filter(|t| t.kind == K::Ident).last(),
                _ => None,
            };
            let v = self.sql_value(cx, scope, e, place);
            match label {
                Some(label) => items.push((label.text(self.text).to_owned(), v, self.span(e))),
                None => self.error(
                    codes::Q010,
                    self.span(e),
                    "name this result: `select { label = … }`",
                ),
            }
        }
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (label, v, span) in items {
            if seen.contains(&label) {
                self.error(
                    codes::Q004,
                    span,
                    format!("two results are named `{label}`: give each its own label"),
                );
                continue;
            }
            seen.push(label.clone());
            let sql = format!("{} AS {}", v.sql, quote(&label));
            let o = Out {
                label,
                ty: v.ty.clone(),
                null: v.null,
                form: v.form.clone(),
            };
            out.push((o, sql, v, span));
        }
        out
    }
}

/// `Option t` for a value that may be `NULL`.
pub fn optional(ty: Type, null: bool) -> Type {
    if null {
        Type::app(Type::prim("Option"), ty)
    } else {
        ty
    }
}
