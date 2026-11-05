//! The instance `Enum T b` the compiler gives each enum, `T` an `untagged
//! type` of literals of type `b`: its methods are the companions `T.value`,
//! `T.parse` and `T.all`, which lowering writes. A program declares none.

use super::declare::Declared;
use super::split;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::env::{Abi, EnumLit, Instance};
use crate::types::lang::{self, Lang};
use crate::types::ty::Type;

impl<'n> Checker<'_> {
    /// The instance of `Enum` for the type `decl` declares, when it is an
    /// enum.
    pub(super) fn enum_instance(&mut self, decl: &'n Node) -> Option<Declared<'n>> {
        let name = self.con_token(decl)?;
        let Some(Abi::Literal(literals)) = self.typing.abis.get(&name) else {
            return None;
        };
        let base = match literals.first()? {
            EnumLit::String(_) => Lang::String,
            EnumLit::Int(_) => Lang::Int,
        };
        let class = lang::table().get(Lang::Enum);
        let module = self.env.module.clone();
        let target = Type::con(&module, &name);
        let head = Type::apps(
            Type::con(&class.module, &class.name),
            [target, Type::lang(base)],
        );
        let instance = Instance {
            module,
            name: self.show(&head),
            vars: Vec::new(),
            context: Vec::new(),
            head,
        };
        let span = self.span(decl);
        let admitted = self.admit(&instance, span);
        admitted.then_some(Declared {
            decl,
            span,
            instance,
            fields: None,
            generic: None,
            enumerates: Some(name),
        })
    }

    /// Reports an `instance` of `Enum` a program writes; whether it is one.
    pub(super) fn written_enum(&mut self, instance: &Instance, span: Span) -> bool {
        let Some(((module, name), _)) = split(&instance.head) else {
            return false;
        };
        if !lang::is(Lang::Enum, module, name) {
            return false;
        }
        let message = "`Enum` is the compiler's: each enum, an `untagged type` of literals, has its instance, and a program declares none".to_owned();
        self.error(codes::C012, span, message);
        true
    }
}
