//! Generated `match`, `if` and blocks. A `match` and an `if` stand on one
//! line, as the rest of generated code; a block has a line for each
//! statement and its value, two columns in from the block around it, so the
//! layout of the parser reads it.

use flick_plugin::{Arm, Decl, Kind, NodeId};

use super::{Printer, Role};

impl Printer<'_> {
    /// `(match e with | p when g -> v …)`.
    pub(super) fn match_on(
        &mut self,
        scrutinee: NodeId,
        arms: &[Arm],
        d: u32,
    ) -> Result<(), String> {
        if arms.is_empty() {
            return Err("a `match` without arms".into());
        }
        self.push("(match ");
        self.node(scrutinee, Role::Expr, d)?;
        self.push(" with");
        for arm in arms {
            self.push(" | ");
            self.node(arm.pattern, Role::Pat, d)?;
            if let Some(guard) = arm.guard {
                self.push(" when ");
                self.node(guard, Role::Expr, d)?;
            }
            self.push(" -> ");
            self.node(arm.body, Role::Expr, d)?;
        }
        self.push(")");
        Ok(())
    }

    /// `(if c then a else b)`.
    pub(super) fn branches(
        &mut self,
        condition: NodeId,
        then: NodeId,
        otherwise: NodeId,
        d: u32,
    ) -> Result<(), String> {
        self.push("(if ");
        self.node(condition, Role::Expr, d)?;
        self.push(" then ");
        self.node(then, Role::Expr, d)?;
        self.push(" else ");
        self.node(otherwise, Role::Expr, d)?;
        self.push(")");
        Ok(())
    }

    /// Each statement on a line of its own, then the value: a `let` or a
    /// signature, a hole of one, or an expression.
    pub(super) fn block(
        &mut self,
        statements: &[NodeId],
        result: NodeId,
        d: u32,
    ) -> Result<(), String> {
        let outer = self.indent;
        self.indent += 2;
        let line = format!("\n{}", " ".repeat(self.indent));
        for &statement in statements {
            let role = match self.arena.nodes.get(statement as usize).map(|n| &n.kind) {
                Some(Kind::Decl(Decl::Let { .. } | Decl::Signature { .. } | Decl::Hole(_))) => {
                    Role::Decl
                }
                Some(Kind::Expr(_)) => Role::Expr,
                _ => {
                    return Err(format!(
                        "statement {statement} of a block is no `let`, signature or expression"
                    ));
                }
            };
            self.push(&line);
            self.node(statement, role, d)?;
        }
        self.push(&line);
        self.node(result, Role::Expr, d)?;
        self.indent = outer;
        Ok(())
    }
}
