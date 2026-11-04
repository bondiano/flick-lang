//! Generated declarations as text: each on a line of its own at column 1,
//! the members of an `instance` one level in, every expression and type in
//! them on one line as the rest of generated code is.

use flick_plugin::{Case, Decl, Hole, HoleKind, Kind, NodeId, Span};

use super::{Names, Printer, Role};

impl Printer<'_> {
    pub(super) fn decl(&mut self, decl: &Decl, depth: u32) -> Result<(), String> {
        let d = depth + 1;
        match decl {
            Decl::Group(decls) => {
                if decls.is_empty() {
                    return Err("a group of no declarations".into());
                }
                for (i, &id) in decls.iter().enumerate() {
                    if i > 0 {
                        self.push("\n");
                    }
                    self.node(id, Role::Decl, d)?;
                }
            }
            Decl::Let {
                name,
                params,
                ty,
                body,
                rec,
            } => {
                self.push(if *rec { "let rec " } else { "let " });
                self.value(name)?;
                for &p in params {
                    self.push(" ");
                    self.node(p, Role::Pat, d)?;
                }
                if let Some(ty) = ty {
                    self.push(" : ");
                    self.node(*ty, Role::Ty, d)?;
                }
                self.push(" = ");
                self.node(*body, Role::Expr, d)?;
            }
            Decl::Signature { name, ty } => {
                self.value(name)?;
                self.push(" : ");
                self.node(*ty, Role::Ty, d)?;
            }
            Decl::Alias { name, params, ty } => {
                self.push("type alias ");
                self.type_head(name, params)?;
                self.push(" = ");
                self.node(*ty, Role::Ty, d)?;
            }
            Decl::Adt {
                name,
                params,
                cases,
                deriving,
            } => self.adt(name, params, cases, deriving, d)?,
            Decl::Instance {
                context,
                head,
                members,
            } => self.instance(context, *head, members, d)?,
            Decl::Hole(span) => self.decl_hole(*span)?,
            Decl::Doc { text, decl } => {
                for line in text.lines() {
                    if line.contains(['\r', '\u{2028}', '\u{2029}']) {
                        return Err("a line of documentation that does not end where it is".into());
                    }
                    self.push("/// ");
                    self.push(line);
                    self.push("\n");
                }
                self.node(*decl, Role::Decl, d)?;
            }
        }
        Ok(())
    }

    /// `type Name params = A fields | … deriving C, …`.
    fn adt(
        &mut self,
        name: &str,
        params: &[String],
        cases: &[Case],
        deriving: &[String],
        d: u32,
    ) -> Result<(), String> {
        if cases.is_empty() {
            return Err(format!("the type `{name}` has no constructor"));
        }
        self.push("type ");
        self.type_head(name, params)?;
        self.push(" =");
        for (i, case) in cases.iter().enumerate() {
            self.push(if i == 0 { " " } else { " | " });
            self.declared(&case.name, Names::Constructor)?;
            for &field in &case.fields {
                self.push(" ");
                self.node(field, Role::Ty, d)?;
            }
        }
        for (i, class) in deriving.iter().enumerate() {
            self.push(if i == 0 { " deriving " } else { ", " });
            self.name(class, Names::Type)?;
        }
        Ok(())
    }

    /// `instance context => head =` and its members, one level in.
    fn instance(
        &mut self,
        context: &[NodeId],
        head: NodeId,
        members: &[NodeId],
        d: u32,
    ) -> Result<(), String> {
        self.push("instance ");
        match context {
            [] => {}
            [one] => {
                self.node(*one, Role::Ty, d)?;
                self.push(" => ");
            }
            several => {
                self.push("(");
                for (i, &c) in several.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.node(c, Role::Ty, d)?;
                }
                self.push(") => ");
            }
        }
        self.node(head, Role::Ty, d)?;
        if members.is_empty() {
            return Ok(());
        }
        self.push(" =");
        for &member in members {
            let method = matches!(
                self.arena.nodes.get(member as usize).map(|n| &n.kind),
                Some(Kind::Decl(Decl::Let { .. } | Decl::Signature { .. }))
            );
            if !method {
                return Err(format!(
                    "member {member} of an instance is no `let` or signature"
                ));
            }
            self.push("\n  ");
            self.node(member, Role::Decl, d)?;
        }
        Ok(())
    }

    /// `Name a b`: a name declared here, unqualified, and its parameters.
    fn type_head(&mut self, name: &str, params: &[String]) -> Result<(), String> {
        self.declared(name, Names::Constructor)?;
        for p in params {
            self.push(" ");
            self.name(p, Names::Label)?;
        }
        Ok(())
    }

    /// The value a `let` or a signature declares: a name, or `Owner.member`,
    /// a companion of the type `Owner`, which only generated code writes as
    /// `Owner$member`.
    fn value(&mut self, name: &str) -> Result<(), String> {
        match name.split_once('.') {
            Some((owner, member)) => {
                self.declared(owner, Names::Constructor)?;
                self.push("$");
                self.name(member, Names::Label)
            }
            None => self.name(name, Names::Label),
        }
    }

    /// A name a declaration declares: of `names`, without a module.
    fn declared(&mut self, name: &str, names: Names) -> Result<(), String> {
        if name.contains('.') {
            return Err(format!("`{name}` is declared here, so it has no module"));
        }
        self.name(name, names)
    }

    /// The placeholder of the user's declaration: a `let` of its own, which
    /// the splice replaces by the declaration at `span`.
    fn decl_hole(&mut self, span: Span) -> Result<(), String> {
        self.within(span, "a hole")?;
        if span.start == span.end {
            return Err("an empty hole".into());
        }
        let at = self.offset();
        self.holes.push((
            at,
            Hole {
                kind: HoleKind::Decl,
                span,
            },
        ));
        self.push(DECL_PLACEHOLDER);
        Ok(())
    }
}

/// The text a hole of a declaration stands as until the splice.
pub const DECL_PLACEHOLDER: &str = "let hole = ()";
