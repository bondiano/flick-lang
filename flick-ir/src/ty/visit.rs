//! Deterministic traversal of every [`TypeRef`] inside a [`Module`] or [`Expr`].

use super::{Constraint, TypeRef};
use crate::{Constructor, Expr, ExprKind, ItemKind, Module, Pat, Stmt};

impl Module {
    /// Calls `f` on every [`TypeRef`] stored in this module, in deterministic order.
    pub fn each_type_ref<'a>(&'a self, f: &mut dyn FnMut(&'a TypeRef)) {
        for decl in &self.types {
            if let Some(alias) = &decl.alias {
                f(alias);
            }
            for ctor in &decl.constructors {
                for c in &ctor.constraints {
                    each_constraint(c, f);
                }
                for field in &ctor.fields {
                    f(field);
                }
                if let Some(args) = &ctor.result_args {
                    for a in args {
                        f(a);
                    }
                }
            }
        }
        for item in &self.items {
            match &item.kind {
                ItemKind::Value {
                    dict_constraints,
                    ty,
                    body,
                    ..
                } => {
                    for c in dict_constraints {
                        each_constraint(c, f);
                    }
                    if let Some(ty) = ty {
                        f(ty);
                    }
                    body.each_type_ref(f);
                }
                ItemKind::Destructure {
                    pattern,
                    binder_types,
                    body,
                } => {
                    each_pat(pattern, f);
                    for (_, ty) in binder_types {
                        f(ty);
                    }
                    body.each_type_ref(f);
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(ty) = &foreign.ty {
                        f(ty);
                    }
                    if let Some(catch) = &foreign.catch {
                        catch.each_type_ref(f);
                    }
                }
                ItemKind::Constructor(c) => each_constructor(c, f),
                ItemKind::Instance { info, methods } => {
                    if let Some((_, Some(ctors))) = &info.derived {
                        for c in ctors {
                            each_constructor(c, f);
                        }
                    }
                    if let Some(generic) = &info.generic {
                        for (c, _) in &generic.cases {
                            each_constructor(c, f);
                        }
                        each_constructor(&generic.inl, f);
                        each_constructor(&generic.inr, f);
                        each_constructor(&generic.con, f);
                        each_constructor(&generic.field, f);
                    }
                    for (_, body) in methods {
                        body.each_type_ref(f);
                    }
                }
                ItemKind::Service { .. } => {}
            }
        }
    }
}

impl Expr {
    /// Calls `f` on every [`TypeRef`] in this expression and its descendants.
    pub fn each_type_ref<'a>(&'a self, f: &mut dyn FnMut(&'a TypeRef)) {
        if let Some(ty) = &self.ty {
            f(ty);
        }
        match &self.kind {
            ExprKind::Global { type_args, .. } | ExprKind::Method { type_args, .. } => {
                for a in type_args {
                    f(a);
                }
            }
            ExprKind::Con(c) => each_constructor(c, f),
            ExprKind::Lambda { params, .. } => {
                for p in params {
                    each_pat(&p.pat, f);
                    if let Some(ty) = &p.ty {
                        f(ty);
                    }
                    for (_, ty) in &p.binder_types {
                        f(ty);
                    }
                }
            }
            ExprKind::Each {
                pat, binder_types, ..
            } => {
                each_pat(pat, f);
                for (_, ty) in binder_types {
                    f(ty);
                }
            }
            ExprKind::Match(_, arms) => {
                for arm in arms {
                    each_pat(&arm.pat, f);
                    for (_, ty) in &arm.binder_types {
                        f(ty);
                    }
                }
            }
            ExprKind::Block(stmts, _) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let {
                            pat, binder_types, ..
                        } => {
                            each_pat(pat, f);
                            for (_, ty) in binder_types {
                                f(ty);
                            }
                        }
                        Stmt::Var { ty, .. } => {
                            if let Some(ty) = ty {
                                f(ty);
                            }
                        }
                        Stmt::Fun {
                            dict_constraints,
                            ty,
                            ..
                        } => {
                            for c in dict_constraints {
                                each_constraint(c, f);
                            }
                            if let Some(ty) = ty {
                                f(ty);
                            }
                        }
                        Stmt::Assign(..) | Stmt::Expr(_) => {}
                    }
                }
            }
            ExprKind::Handle(h) => {
                for arm in &h.errors {
                    each_pat(&arm.pat, f);
                    for (_, ty) in &arm.binder_types {
                        f(ty);
                    }
                }
            }
            _ => {}
        }
        self.each_child(&mut |child| child.each_type_ref(f));
    }
}

fn each_constraint<'a>(c: &'a Constraint, f: &mut dyn FnMut(&'a TypeRef)) {
    for a in &c.args {
        f(a);
    }
}

fn each_constructor<'a>(c: &'a Constructor, f: &mut dyn FnMut(&'a TypeRef)) {
    for field in &c.fields {
        f(field);
    }
    for arg in &c.type_args {
        f(arg);
    }
}

fn each_pat<'a>(pat: &'a Pat, f: &mut dyn FnMut(&'a TypeRef)) {
    match pat {
        Pat::Con(c, items) => {
            each_constructor(c, f);
            for p in items {
                each_pat(p, f);
            }
        }
        Pat::Tuple(items) | Pat::Or(items) => {
            for p in items {
                each_pat(p, f);
            }
        }
        Pat::List(items, rest) => {
            for p in items {
                each_pat(p, f);
            }
            if let Some(rest) = rest {
                each_pat(rest, f);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                each_pat(p, f);
            }
        }
        Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
    }
}

impl Module {
    /// Calls `f` mutably on every [`TypeRef`] stored in this module.
    pub fn each_type_ref_mut(&mut self, f: &mut dyn FnMut(&mut TypeRef)) {
        for decl in &mut self.types {
            if let Some(alias) = &mut decl.alias {
                f(alias);
            }
            for ctor in &mut decl.constructors {
                for c in &mut ctor.constraints {
                    each_constraint_mut(c, f);
                }
                for field in &mut ctor.fields {
                    f(field);
                }
                if let Some(args) = &mut ctor.result_args {
                    for a in args {
                        f(a);
                    }
                }
            }
        }
        for item in &mut self.items {
            match &mut item.kind {
                ItemKind::Value {
                    dict_constraints,
                    ty,
                    body,
                    ..
                } => {
                    for c in dict_constraints {
                        each_constraint_mut(c, f);
                    }
                    if let Some(ty) = ty {
                        f(ty);
                    }
                    body.each_type_ref_mut(f);
                }
                ItemKind::Destructure {
                    pattern,
                    binder_types,
                    body,
                } => {
                    each_pat_mut(pattern, f);
                    for (_, ty) in binder_types {
                        f(ty);
                    }
                    body.each_type_ref_mut(f);
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(ty) = &mut foreign.ty {
                        f(ty);
                    }
                    if let Some(catch) = &mut foreign.catch {
                        catch.each_type_ref_mut(f);
                    }
                }
                ItemKind::Constructor(c) => each_constructor_mut(c, f),
                ItemKind::Instance { info, methods } => {
                    if let Some((_, Some(ctors))) = &mut info.derived {
                        for c in ctors {
                            each_constructor_mut(c, f);
                        }
                    }
                    if let Some(generic) = &mut info.generic {
                        for (c, _) in &mut generic.cases {
                            each_constructor_mut(c, f);
                        }
                        each_constructor_mut(&mut generic.inl, f);
                        each_constructor_mut(&mut generic.inr, f);
                        each_constructor_mut(&mut generic.con, f);
                        each_constructor_mut(&mut generic.field, f);
                    }
                    for (_, body) in methods {
                        body.each_type_ref_mut(f);
                    }
                }
                ItemKind::Service { .. } => {}
            }
        }
    }
}

impl Expr {
    /// Calls `f` mutably on every [`TypeRef`] in this expression and its descendants.
    pub fn each_type_ref_mut(&mut self, f: &mut dyn FnMut(&mut TypeRef)) {
        if let Some(ty) = &mut self.ty {
            f(ty);
        }
        match &mut self.kind {
            ExprKind::Global { type_args, .. } | ExprKind::Method { type_args, .. } => {
                for a in type_args {
                    f(a);
                }
            }
            ExprKind::Con(c) => each_constructor_mut(c, f),
            ExprKind::Lambda { params, .. } => {
                for p in params {
                    each_pat_mut(&mut p.pat, f);
                    if let Some(ty) = &mut p.ty {
                        f(ty);
                    }
                    for (_, ty) in &mut p.binder_types {
                        f(ty);
                    }
                }
            }
            ExprKind::Each {
                pat, binder_types, ..
            } => {
                each_pat_mut(pat, f);
                for (_, ty) in binder_types {
                    f(ty);
                }
            }
            ExprKind::Match(_, arms) => {
                for arm in arms {
                    each_pat_mut(&mut arm.pat, f);
                    for (_, ty) in &mut arm.binder_types {
                        f(ty);
                    }
                }
            }
            ExprKind::Block(stmts, _) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let {
                            pat, binder_types, ..
                        } => {
                            each_pat_mut(pat, f);
                            for (_, ty) in binder_types {
                                f(ty);
                            }
                        }
                        Stmt::Var { ty, .. } => {
                            if let Some(ty) = ty {
                                f(ty);
                            }
                        }
                        Stmt::Fun {
                            dict_constraints,
                            ty,
                            ..
                        } => {
                            for c in dict_constraints {
                                each_constraint_mut(c, f);
                            }
                            if let Some(ty) = ty {
                                f(ty);
                            }
                        }
                        Stmt::Assign(..) | Stmt::Expr(_) => {}
                    }
                }
            }
            ExprKind::Handle(h) => {
                for arm in &mut h.errors {
                    each_pat_mut(&mut arm.pat, f);
                    for (_, ty) in &mut arm.binder_types {
                        f(ty);
                    }
                }
            }
            _ => {}
        }
        self.each_child_mut(&mut |child| child.each_type_ref_mut(f));
    }
}

fn each_constraint_mut(c: &mut Constraint, f: &mut dyn FnMut(&mut TypeRef)) {
    for a in &mut c.args {
        f(a);
    }
}

fn each_constructor_mut(c: &mut Constructor, f: &mut dyn FnMut(&mut TypeRef)) {
    for field in &mut c.fields {
        f(field);
    }
    for arg in &mut c.type_args {
        f(arg);
    }
}

fn each_pat_mut(pat: &mut Pat, f: &mut dyn FnMut(&mut TypeRef)) {
    match pat {
        Pat::Con(c, items) => {
            each_constructor_mut(c, f);
            for p in items {
                each_pat_mut(p, f);
            }
        }
        Pat::Tuple(items) | Pat::Or(items) => {
            for p in items {
                each_pat_mut(p, f);
            }
        }
        Pat::List(items, rest) => {
            for p in items {
                each_pat_mut(p, f);
            }
            if let Some(rest) = rest {
                each_pat_mut(rest, f);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                each_pat_mut(p, f);
            }
        }
        Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
    }
}
