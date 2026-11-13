//! Declarations, references and dictionaries: the small pieces of code the
//! rest of the generator puts together.

use std::mem;

use flick_pretty::{Document, docvec};

use super::doc::{After, Before, Form, Head, Js, Stmt, array, call, text_flags};
use super::{Gen, names};
#[cfg(feature = "native-project")]
use crate::source::Span;
use flick_ir::Dict;

impl<'d> Gen<'_, 'd> {
    pub(super) fn temp(&mut self) -> String {
        self.temps += 1;
        format!("$t{}", self.temps)
    }

    /// A name, a literal or a module member, as written.
    pub(super) fn leaf(&self, text: &str) -> Js<'d> {
        Js::text(self.a, self.a.alloc_str(text))
    }

    /// A string literal.
    pub(super) fn string(&self, text: &str) -> Js<'d> {
        self.leaf(&names::string(text))
    }

    /// An arrow's parameter list: in TypeScript every parameter and result is `any`.
    pub(super) fn arrow(&self, params: &str) -> Document<'d> {
        let a = self.a;
        if !self.options.typescript() {
            return docvec![a, "(", a.alloc_str(params), ") =>"];
        }
        let typed: Vec<String> = params
            .split(", ")
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}: any"))
            .collect();
        docvec![a, "(", typed.join(", "), "): any =>"]
    }

    /// The arrow from `params` to `body`.
    pub(super) fn lambda_of(&self, params: &str, body: Js<'d>) -> Js<'d> {
        self.arrow_to(self.arrow(params), body)
    }

    /// `() => body`, untyped in TypeScript too.
    pub(super) fn thunk(&self, body: Js<'d>) -> Js<'d> {
        self.arrow_to(self.a.text("() =>"), body)
    }

    /// The arrow `head body`, where `head` is its parameter list and `=>`.
    pub(super) fn arrow_to(&self, head: Document<'d>, body: Js<'d>) -> Js<'d> {
        let a = self.a;
        let doc = docvec![a, head, " ", body.at(a, Before::Arrow, After::Delimiter)];
        // a body in parentheses keeps them when the arrow is not delimited
        let open = body.tail.is_some() || body.paren && body.head != Head::Brace;
        let tail = open.then(|| docvec![a, head, " ", body.at(a, Before::Arrow, After::Other)]);
        Js {
            tail,
            ..Js::code(doc, Head::Paren, &[body])
        }
    }

    /// `const name = value;`, `any` in TypeScript.
    pub(super) fn constant(&self, name: &str, value: Js<'d>) -> Stmt<'d> {
        let a = self.a;
        let ty = if self.options.typescript() {
            ": any"
        } else {
            ""
        };
        let value_doc = value.at(a, Before::Assign, After::Semicolon);
        Stmt::new(
            docvec![a, "const ", a.alloc_str(name), ty, " = ", value_doc, ";"],
            value.flags | text_flags(name),
        )
    }

    pub(super) fn variable(&self, name: &str, value: Js<'d>) -> Stmt<'d> {
        let a = self.a;
        let ty = if self.options.typescript() {
            ": any"
        } else {
            ""
        };
        Stmt::new(
            docvec![
                a,
                "let ",
                a.alloc_str(name),
                ty,
                " = ",
                value.at(a, Before::Assign, After::Semicolon),
                ";"
            ],
            value.flags | text_flags(name),
        )
    }

    /// A native generator; its yields stay inside its own body.
    pub(super) fn generator_function(&self, params: &str, stmts: &[Stmt<'d>]) -> Js<'d> {
        let a = self.a;
        let params = params
            .split(", ")
            .filter(|p| !p.is_empty())
            .map(|p| {
                if self.options.typescript() {
                    format!("{p}: any")
                } else {
                    p.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let returns = if self.options.typescript() {
            ": globalThis.Generator<any, any, any>"
        } else {
            ""
        };
        Js::code(
            docvec![
                a,
                "function* (",
                params,
                ")",
                returns,
                " ",
                super::doc::block(a, stmts)
            ],
            Head::Ident,
            &[],
        )
        .with((super::doc::stmt_flags(stmts) & !super::doc::YIELDS) | super::doc::MULTILINE)
    }

    /// An exported `const`: in TypeScript with its projected type.
    pub(super) fn export(&self, name: &str, value: Js<'d>) -> Stmt<'d> {
        self.export_marked(name, None, value)
    }

    /// An exported `const`, its name after the cursor `mark`.
    pub(super) fn export_marked(
        &self,
        name: &str,
        mark: Option<Document<'d>>,
        value: Js<'d>,
    ) -> Stmt<'d> {
        let a = self.a;
        // called from TypeScript, outside a Flick sync context, it runs as an Effect
        let value = match self.sync_exports.get(name) {
            Some(stages) => call(
                a,
                self.leaf("$rt.syncExport"),
                &[value, self.leaf(&stages.to_string())],
            ),
            None => value,
        };
        let value = self.pure(value);
        let ty = self.annotation(name);
        let mark = mark.unwrap_or_else(|| a.empty());
        let doc = docvec![
            a,
            "export const ",
            mark,
            a.alloc_str(name),
            ty,
            " = ",
            value.at(a, Before::Assign, After::Semicolon),
            ";"
        ];
        Stmt::new(doc, value.flags | text_flags(name))
    }

    /// A top-level `value` that is a call or an immediately run arrow,
    /// marked `/*#__PURE__*/`: top-level initialization is pure (spec 11),
    /// so a bundler may drop it when nothing uses it.
    pub(super) fn pure(&self, value: Js<'d>) -> Js<'d> {
        if !value.is_pure() {
            return value;
        }
        let a = self.a;
        let doc = docvec![
            a,
            "/*#__PURE__*/ ",
            value.at(a, Before::Other, After::Semicolon)
        ];
        Js::code(doc, Head::Other, &[value])
    }

    /// The type an exported constant is declared with.
    fn annotation(&self, name: &str) -> Document<'d> {
        if let Some(ty) = self.annotations.get(name) {
            return docvec![self.a, ": ", self.a.alloc_str(ty)];
        }
        let _ = name;
        if self.options.typescript() {
            self.a.text(": any")
        } else {
            self.a.empty()
        }
    }

    /// `let` of variables assigned later.
    pub(super) fn declare(&self, names: &str) -> Stmt<'d> {
        let names = if self.options.typescript() {
            let typed: Vec<String> = names.split(", ").map(|n| format!("{n}: any")).collect();
            typed.join(", ")
        } else {
            names.to_owned()
        };
        let flags = text_flags(&names);
        Stmt::new(docvec![self.a, "let ", names, ";"], flags)
    }

    /// A statement of one line.
    pub(super) fn line(&self, text: &str) -> Stmt<'d> {
        Stmt::new(self.a.text(self.a.alloc_str(text)), text_flags(text))
    }

    pub(super) fn rt(&mut self, name: &str) -> Js<'d> {
        self.runtime = true;
        self.leaf(&format!("$rt.{name}"))
    }

    pub(super) fn effect(&mut self, name: &str) -> Js<'d> {
        self.effect = true;
        self.leaf(&format!("Effect.{name}"))
    }

    #[cfg(feature = "native-project")]
    /// A cursor that maps the generated position to `span`.
    pub(super) fn mark(&mut self, span: Span) -> Document<'d> {
        self.spans.push(span);
        self.a.cursor(self.spans.len() - 1)
    }

    /// A top-level value of `module` by its JS name.
    pub(super) fn global(&mut self, module: &str, name: &str) -> Js<'d> {
        let relocated = (self.location)(module, name);
        let target = relocated.as_deref().unwrap_or(module);
        if target == self.physical {
            self.leaf(name)
        } else {
            self.imports.insert(target.to_owned());
            let alias = self.alias(target);
            self.leaf(&format!("{alias}.{name}"))
        }
    }

    /// The name `module` is imported under: in debug its own (`Ui`,
    /// `Ui_Event`) unless a constructor of this module, a JS global or
    /// another import has it; `$m_Ui` otherwise.
    pub(super) fn alias(&mut self, module: &str) -> String {
        if let Some(alias) = self.aliases.get(module) {
            return alias.clone();
        }
        let readable = module.replace('.', "_");
        let free = !self.options.release
            && !names::is_global(&readable)
            && !self.constructors.contains(&readable)
            && !self.aliases.values().any(|a| *a == readable);
        let alias = if free {
            readable
        } else {
            names::module_alias(module, 'm')
        };
        self.aliases.insert(module.to_owned(), alias.clone());
        alias
    }

    /// The member `name` of `object`, `object.name`.
    pub(super) fn member(&self, object: Js<'d>, name: &str) -> Js<'d> {
        if let Some(text) = object.atom() {
            return self.leaf(&format!("{text}.{name}"));
        }
        let a = self.a;
        let doc = docvec![
            a,
            object.at(a, Before::Other, After::Other),
            ".",
            a.alloc_str(name)
        ];
        let member = object.starting(doc, &[object]);
        match object.form {
            Form::Call | Form::Member => Js {
                form: Form::Member,
                ..member
            },
            _ => member,
        }
    }

    pub(super) fn dict(&mut self, dict: &Dict) -> Js<'d> {
        match dict {
            Dict::Instance { module, name, args } => {
                let head = self.global(module, &names::instance(name));
                if args.is_empty() {
                    head
                } else {
                    let args: Vec<Js<'d>> = args.iter().map(|a| self.dict(a)).collect();
                    call(self.a, head, &args)
                }
            }
            Dict::Given(shown) => {
                let param = self
                    .givens
                    .iter()
                    .rev()
                    .find(|(g, _)| g == shown)
                    .map_or_else(
                        || panic!("dictionary `{shown}` is in scope"),
                        |(_, p)| p.clone(),
                    );
                self.leaf(&param)
            }
            Dict::Value { module, name } => self.global(module, &names::ident(name)),
            Dict::Opened { binder, field } => {
                self.leaf(&format!("{}.{field}", names::ident(binder)))
            }
            Dict::Field(local) => self.leaf(&names::ident(local)),
            Dict::Site { place, definition } => {
                let a = self.a;
                let place = self.string(place);
                let definition = self.string(definition);
                super::doc::object(
                    a,
                    &[
                        docvec![a, "place: ", place.doc],
                        docvec![a, "definition: ", definition.doc],
                    ],
                    &[place, definition],
                )
            }
            Dict::Super { dict, class } => {
                let dict = self.dict(dict);
                self.member(dict, &format!("${class}"))
            }
            Dict::Structural { class, fields } => {
                let fields: Vec<Js<'d>> = fields.iter().map(|f| self.dict(f)).collect();
                let structural = self.rt("structural");
                let fields = array(self.a, &fields);
                call(self.a, structural, &[self.string(class), fields])
            }
            Dict::Primitive { labels, .. } => {
                let labels: Vec<Js<'d>> = labels.iter().map(|l| self.string(l)).collect();
                array(self.a, &labels)
            }
            Dict::Error => self.leaf("undefined"),
        }
    }

    /// Binds the dictionaries `shown` to fresh parameters; the parameter list.
    pub(super) fn bind_givens(&mut self, shown: &[String]) -> String {
        let params: Vec<String> = shown
            .iter()
            .map(|g| {
                let p = format!("$d{}", self.givens.len());
                self.givens.push((g.clone(), p.clone()));
                p
            })
            .collect();
        params.join(", ")
    }

    /// Runs `f` with an empty statement list; its statements and result.
    pub(super) fn block<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (Vec<Stmt<'d>>, T) {
        let saved = mem::take(&mut self.buf);
        let result = f(self);
        (mem::replace(&mut self.buf, saved), result)
    }
}
