//! The code of a table: its row type, the signature of its companion and
//! the companion itself, the columns as the runtime reads them.
//!
//! ```flick
//! type alias Users = { id : UserId; email : Option String }
//! Users.table : Sql.Table "users" { id : Sql.Column UserId Sql.NonNull Sql.Generated; … }
//! let Users.table = Sql.make_table ({ name = "users"; columns = [
//!   Sql.Derive.via "id" "bigint" false (user_id : PureCodec BigInt UserId),
//!   Sql.Derive.plain "email" "text" true (Sql.Derive.same : Sql.Derive.Same String _),
//! ] } : _)
//! ```
//!
//! The `_` after a column's type is the fact `check` compares with the type
//! of its SQL type; the one of the whole table makes `check` run.

use flick_plugin::{Arena, Block, Builder, Compiler, Decl, Expr, Field, NodeId, Ty};

use crate::table::{Column, Table};

/// The code of `table`, the columns of an enum of its SQL type's values
/// read through the enum's `parse`.
pub fn arena(block: &Block, table: &Table, compiler: &mut dyn Compiler) -> Arena {
    let mut b = Builder::new(block.whole());
    let user = |b: &mut Builder, c: &Column| b.at(c.ty).ty(Ty::Hole(c.ty));
    let named = |b: &mut Builder, n: &str| b.ty(Ty::Name(n.to_owned()));
    let mut row = Vec::new();
    let mut shapes = Vec::new();
    let mut runtime = Vec::new();
    for c in &table.columns {
        let ty = user(&mut b, c);
        let value = if c.nullable {
            let option = named(&mut b, "Option");
            b.ty(Ty::Apply {
                function: option,
                arguments: vec![ty],
            })
        } else {
            ty
        };
        row.push(Field {
            label: c.name.clone(),
            value,
        });
        let column = named(&mut b, "Sql.Column");
        let null = named(
            &mut b,
            if c.nullable {
                "Sql.Nullable"
            } else {
                "Sql.NonNull"
            },
        );
        let write = named(&mut b, &format!("Sql.{}", c.write));
        let ty = user(&mut b, c);
        let shape = b.ty(Ty::Apply {
            function: column,
            arguments: vec![ty, null, write],
        });
        shapes.push(Field {
            label: c.name.clone(),
            value: shape,
        });
        runtime.push(column_code(&mut b, block, c, compiler));
    }
    let row = b.ty(Ty::Record(row));
    let alias = b.decl(Decl::Alias {
        name: table.name.clone(),
        params: Vec::new(),
        ty: row,
    });
    let companion = format!("{}.table", table.name);
    let sql_table = named(&mut b, "Sql.Table");
    let sql_name = b.ty(Ty::Text(table.sql.clone()));
    let shapes = b.ty(Ty::Record(shapes));
    let signature = b.ty(Ty::Apply {
        function: sql_table,
        arguments: vec![sql_name, shapes],
    });
    let signature = b.decl(Decl::Signature {
        name: companion.clone(),
        ty: signature,
    });
    let name = b.expr(Expr::Text(table.sql.clone()));
    let columns = b.expr(Expr::Items(runtime));
    let record = b.expr(Expr::Record(vec![
        Field {
            label: "name".into(),
            value: name,
        },
        Field {
            label: "columns".into(),
            value: columns,
        },
    ]));
    let any = b.at(block.whole()).ty(Ty::Wildcard);
    let record = b.expr(Expr::Annot {
        expr: record,
        ty: any,
    });
    let body = call(&mut b, "Sql.make_table", vec![record]);
    let value = b.decl(Decl::Let {
        name: companion,
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    });
    let group = b.group(vec![alias, signature, value]);
    b.finish(group)
}

/// What the runtime reads of the column `c`: its name, SQL kind, whether it
/// may be `NULL` and its codec, with the evidence of its type.
fn column_code(b: &mut Builder, block: &Block, c: &Column, compiler: &mut dyn Compiler) -> NodeId {
    b.at(c.line);
    let name = b.expr(Expr::Text(c.name.clone()));
    let kind = b.expr(Expr::Text(c.sql.kind.to_owned()));
    let nullable = b.expr(Expr::Boolean(c.nullable));
    let base = base(b, c);
    let written = block.text_of(c.ty).trim();
    let enumerates = c.via.is_none()
        && written
            .split('.')
            .all(|w| w.starts_with(char::is_uppercase))
        && compiler.literal_of(written).as_deref() == Some(c.sql.flick);
    let codec = match c.via {
        Some(via) => Some(b.at(via).expr(Expr::Hole(via))),
        // an enum of the SQL type's values: a value the enum does not know
        // is no value of the column
        None if enumerates => {
            let parse = b.expr(Expr::Name(format!("{written}.parse")));
            let value = b.expr(Expr::Name(format!("{written}.value")));
            Some(call(b, "Sql.codec", vec![parse, value]))
        }
        None => None,
    };
    if let Some(codec) = codec {
        let pure = b.ty(Ty::Name("PureCodec".into()));
        let ty = b.at(c.ty).ty(Ty::Hole(c.ty));
        let annotated = b.ty(Ty::Apply {
            function: pure,
            arguments: vec![base, ty],
        });
        let codec = b.expr(Expr::Annot {
            expr: codec,
            ty: annotated,
        });
        return call(b, "Sql.Derive.via", vec![name, kind, nullable, codec]);
    }
    let same = b.expr(Expr::Name("Sql.Derive.same".into()));
    let witness = b.ty(Ty::Name("Sql.Derive.Same".into()));
    let ty = b.at(c.ty).ty(Ty::Hole(c.ty));
    let fact = b.at(c.ty).ty(Ty::Wildcard);
    let shape = b.ty(Ty::Apply {
        function: witness,
        arguments: vec![ty, fact],
    });
    let same = b.expr(Expr::Annot {
        expr: same,
        ty: shape,
    });
    call(b, "Sql.Derive.plain", vec![name, kind, nullable, same])
}

/// The Flick type of the column's SQL type, as a column of it holds.
fn base(b: &mut Builder, c: &Column) -> NodeId {
    let name = if c.sql.flick == "BigDecimal" {
        "BigDecimal.BigDecimal"
    } else {
        c.sql.flick
    };
    b.ty(Ty::Name(name.to_owned()))
}

fn call(b: &mut Builder, f: &str, arguments: Vec<NodeId>) -> NodeId {
    let function = b.expr(Expr::Name(f.to_owned()));
    b.expr(Expr::Apply {
        function,
        arguments,
    })
}
