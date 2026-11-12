//! Generated code as text: an arena of core syntax printed on one line,
//! every compound in parentheses so that neither layout nor precedence
//! matters, with where each node and each hole ended up; declarations each
//! on a line of their own. The arena comes from a plugin, or from a cache
//! of one, so everything is checked first.

mod control;
mod decl;

use flick_plugin::{Arena, Expr, Field, Hole, HoleKind, Kind, NodeId, Pat, Place, Span, Ty};

pub use decl::DECL_PLACEHOLDER;

use super::source::{in_signature, signed};
use crate::source::FileId;
use crate::syntax::heads::OPEN;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::lexer::{lex, lex_generated};

/// The most nodes, the deepest nesting and the longest text of generated code.
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: u32 = 200;
const MAX_TEXT: usize = 1 << 20;

/// Generated code as text.
#[derive(Debug, PartialEq)]
pub struct Rendered {
    pub text: String,
    /// The range of `text` each node took, by every place it is used, and the
    /// span of the body it comes from.
    pub origins: Vec<((u32, u32), Span)>,
    /// The offset in `text` of the placeholder of each hole, and the hole.
    pub holes: Vec<(u32, Hole)>,
}

/// `arena` as text, for a block whose body is `body_len` bytes long and that
/// stands at `place`: an expression, or declarations. `aliases` are the
/// modules the file imports only under another name, each with that name:
/// generated code names a module by its path, which the file may not.
pub fn render(
    arena: &Arena,
    body_len: u32,
    place: Place,
    aliases: &[(String, String)],
) -> Result<Rendered, String> {
    if arena.nodes.len() > MAX_NODES {
        return Err(format!("more than {MAX_NODES} nodes"));
    }
    let mut out = Printer {
        arena,
        body_len,
        aliases,
        text: String::new(),
        origins: Vec::new(),
        holes: Vec::new(),
        indent: 0,
    };
    let role = match place {
        Place::Expression => Role::Expr,
        Place::Declaration => Role::Decl,
    };
    out.node(arena.root, role, 0)?;
    // the signature above the block is no part of its body
    let mut holes: Vec<Hole> = (out.holes.iter())
        .map(|(_, h)| *h)
        .filter(|h| in_signature(*h).is_none())
        .collect();
    if place == Place::Expression && out.holes.iter().any(|(_, h)| in_signature(*h).is_some()) {
        return Err("an expression takes no type of the signature above its `let`".into());
    }
    holes.sort_by_key(|h| (h.span.start, h.span.end));
    // a type may stand in several places, as in a signature and an annotation,
    // and a pattern too, as the props of a component its setup and its render
    // bind; an expression or a declaration runs or declares once
    holes.dedup_by(|b, a| a == b && matches!(a.kind, HoleKind::Ty | HoleKind::Pat));
    if holes.windows(2).any(|w| w[1].span.start < w[0].span.end) {
        return Err("two holes overlap or one is used twice".into());
    }
    Ok(Rendered {
        text: out.text,
        origins: out.origins,
        holes: out.holes,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Expr,
    /// The body of a `fun` or a `let`: an expression, or a block.
    Body,
    Ty,
    Pat,
    Decl,
}

struct Printer<'a> {
    arena: &'a Arena,
    body_len: u32,
    aliases: &'a [(String, String)],
    text: String,
    origins: Vec<((u32, u32), Span)>,
    holes: Vec<(u32, Hole)>,
    /// The column of the lines of the block being printed.
    indent: usize,
}

impl Printer<'_> {
    fn node(&mut self, id: NodeId, role: Role, depth: u32) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(format!("nested deeper than {MAX_DEPTH}"));
        }
        let node = self
            .arena
            .nodes
            .get(id as usize)
            .ok_or_else(|| format!("no node {id}"))?;
        if node.kind.children().iter().any(|&child| child >= id) {
            return Err(format!("node {id} refers to a node after it"));
        }
        self.within(node.origin, "the origin of a node")?;
        let start = self.offset();
        match (&node.kind, role) {
            (Kind::Expr(expr), Role::Expr | Role::Body) => self.expr(expr, role, depth)?,
            (Kind::Ty(ty), Role::Ty) => self.ty(ty, depth)?,
            (Kind::Pat(pat), Role::Pat) => self.pat(pat, depth)?,
            (Kind::Decl(decl), Role::Decl) => self.decl(decl, depth)?,
            (_, role) => return Err(format!("node {id} is not a {role:?}")),
        }
        if self.text.len() > MAX_TEXT {
            return Err(format!("longer than {MAX_TEXT} bytes"));
        }
        self.origins.push(((start, self.offset()), node.origin));
        Ok(())
    }

    fn expr(&mut self, expr: &Expr, role: Role, depth: u32) -> Result<(), String> {
        let d = depth + 1;
        match expr {
            Expr::Name(name) => self.name(name, Names::Value)?,
            Expr::Number(n) => self.number(*n)?,
            Expr::Text(text) => self.string(text),
            Expr::Boolean(b) => self.push(if *b { "true" } else { "false" }),
            Expr::Unit => self.push("()"),
            Expr::Apply {
                function,
                arguments,
            } => self.apply(*function, arguments, Role::Expr, d)?,
            Expr::Lambda { params, body } => {
                if params.is_empty() {
                    return Err("a lambda without parameters".into());
                }
                self.push("(fun");
                for &p in params {
                    self.push(" ");
                    self.node(p, Role::Pat, d)?;
                }
                self.push(" -> ");
                self.node(*body, Role::Body, d)?;
                self.push(")");
            }
            Expr::Record(fields) => self.record(fields, " = ", Role::Expr, d)?,
            Expr::Get { record, label } => {
                self.push("(");
                self.node(*record, Role::Expr, d)?;
                self.push(").");
                self.name(label, Names::Label)?;
            }
            Expr::Items(items) => self.sequence("[", items, "]", Role::Expr, d)?,
            Expr::Tuple(items) => {
                if items.len() < 2 {
                    return Err("a tuple of fewer than two items".into());
                }
                self.sequence("(", items, ")", Role::Expr, d)?;
            }
            Expr::Annot { expr, ty } => {
                self.push("(");
                self.node(*expr, Role::Expr, d)?;
                self.push(" : ");
                self.node(*ty, Role::Ty, d)?;
                self.push(")");
            }
            Expr::Hole(span) => self.hole(HoleKind::Expr, *span, "hole")?,
            Expr::Lines(span) => self.hole(HoleKind::Lines, *span, "hole")?,
            Expr::Concat(items) => {
                if items.len() < 2 {
                    return Err("a concatenation of fewer than two items".into());
                }
                self.push("(");
                for (i, &item) in items.iter().enumerate() {
                    if i > 0 {
                        self.push(" ++ ");
                    }
                    self.node(item, Role::Expr, d)?;
                }
                self.push(")");
            }
            Expr::Match { scrutinee, arms } => self.match_on(*scrutinee, arms, d)?,
            Expr::If {
                condition,
                then,
                otherwise,
            } => self.branches(*condition, *then, *otherwise, d)?,
            Expr::Block { .. } if role != Role::Body => {
                return Err("a block is only the body of a `fun` or a `let`".into());
            }
            Expr::Block { statements, result } => self.block(statements, *result, d)?,
            Expr::Open {
                module,
                child,
                body,
            } => {
                // the node ends with its own `)`, so that a span of it is the
                // block's and not the body's
                self.push(OPEN);
                self.push(" ");
                // by the name the file imports it under, as a name of it
                self.name(module, Names::Module)?;
                if let Some(child) = child {
                    self.push(" ");
                    self.name(child, Names::Type)?;
                }
                self.push(" (");
                self.node(*body, Role::Expr, d)?;
                self.push(")");
            }
        }
        Ok(())
    }

    fn ty(&mut self, ty: &Ty, depth: u32) -> Result<(), String> {
        let d = depth + 1;
        match ty {
            Ty::Name(name) => self.name(name, Names::Type)?,
            Ty::Apply {
                function,
                arguments,
            } => self.apply(*function, arguments, Role::Ty, d)?,
            Ty::Record(fields) => self.record(fields, " : ", Role::Ty, d)?,
            Ty::Function { input, output } => {
                self.push("(");
                self.node(*input, Role::Ty, d)?;
                self.push(" -> ");
                self.node(*output, Role::Ty, d)?;
                self.push(")");
            }
            Ty::Text(text) => self.string(text),
            Ty::Wildcard => self.push("_"),
            Ty::Hole(span) => self.hole(HoleKind::Ty, *span, "Hole")?,
            Ty::Signed(span) => {
                if span.start >= span.end {
                    return Err("an empty hole".into());
                }
                self.placeholder(signed(*span), "Hole");
            }
            Ty::Tuple(items) => {
                if items.len() < 2 {
                    return Err("a tuple of fewer than two items".into());
                }
                self.sequence("(", items, ")", Role::Ty, d)?;
            }
            Ty::Forall { vars, body } => {
                if vars.is_empty() {
                    return Err("a `forall` of no variables".into());
                }
                self.push("(forall");
                for v in vars {
                    self.push(" ");
                    self.name(v, Names::Label)?;
                }
                self.push(". ");
                self.node(*body, Role::Ty, d)?;
                self.push(")");
            }
            Ty::Row(entries) => {
                self.push("{");
                for (i, e) in entries.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.node(*e, Role::Ty, d)?;
                }
                self.push("}");
            }
        }
        Ok(())
    }

    fn pat(&mut self, pat: &Pat, depth: u32) -> Result<(), String> {
        let d = depth + 1;
        match pat {
            Pat::Var(name) => self.name(name, Names::Label)?,
            Pat::Wildcard => self.push("_"),
            Pat::Con { name, arguments } if arguments.is_empty() => {
                self.name(name, Names::Constructor)?;
            }
            Pat::Con { name, arguments } => {
                self.push("(");
                self.name(name, Names::Constructor)?;
                for &a in arguments {
                    self.push(" ");
                    self.node(a, Role::Pat, d)?;
                }
                self.push(")");
            }
            Pat::Record(fields) => self.record(fields, " = ", Role::Pat, d)?,
            Pat::Hole(span) => self.hole(HoleKind::Pat, *span, "hole")?,
            Pat::Tuple(items) => {
                if items.len() < 2 {
                    return Err("a tuple of fewer than two items".into());
                }
                self.sequence("(", items, ")", Role::Pat, d)?;
            }
        }
        Ok(())
    }

    /// `(f a b)`, or `f` alone without arguments.
    fn apply(&mut self, f: NodeId, args: &[NodeId], role: Role, d: u32) -> Result<(), String> {
        if args.is_empty() {
            return self.node(f, role, d);
        }
        self.push("(");
        self.node(f, role, d)?;
        for &a in args {
            self.push(" ");
            self.node(a, role, d)?;
        }
        self.push(")");
        Ok(())
    }

    fn record(&mut self, fields: &[Field], sep: &str, role: Role, d: u32) -> Result<(), String> {
        // `{}` is the type of no fields; as a value it is no record
        if fields.is_empty() && role == Role::Ty {
            self.push("{}");
            return Ok(());
        }
        if fields.is_empty() {
            return Err("an empty record".into());
        }
        self.push("{ ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push("; ");
            }
            self.name(&field.label, Names::Label)?;
            self.push(sep);
            self.node(field.value, role, d)?;
        }
        self.push(" }");
        Ok(())
    }

    fn sequence(
        &mut self,
        open: &str,
        items: &[NodeId],
        close: &str,
        role: Role,
        d: u32,
    ) -> Result<(), String> {
        self.push(open);
        for (i, &item) in items.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.node(item, role, d)?;
        }
        self.push(close);
        Ok(())
    }

    /// The placeholder of a hole: a name in parentheses, which the splice
    /// replaces by the user's code at `span`.
    fn hole(&mut self, kind: HoleKind, span: Span, placeholder: &str) -> Result<(), String> {
        self.within(span, "a hole")?;
        if span.start == span.end {
            return Err("an empty hole".into());
        }
        self.placeholder(Hole { kind, span }, placeholder);
        Ok(())
    }

    fn placeholder(&mut self, hole: Hole, placeholder: &str) {
        self.push("(");
        let at = self.offset();
        self.holes.push((at, hole));
        self.push(placeholder);
        self.push(")");
    }

    fn within(&self, span: Span, what: &str) -> Result<(), String> {
        if span.start <= span.end && span.end <= self.body_len {
            Ok(())
        } else {
            Err(format!(
                "{what} at {}..{} is outside the body",
                span.start, span.end
            ))
        }
    }

    fn name(&mut self, name: &str, names: Names) -> Result<(), String> {
        if !names.accept(name) {
            return Err(format!("`{name}` is not {}", names.describe()));
        }
        // shortcut: a built-in module hidden by an alias, as `import Other as List`, stays hidden; give generated code its own qualifiers if a plugin needs one
        let imports = self.aliases;
        let renamed = (imports.iter())
            .filter_map(|(module, alias)| {
                let rest = name.strip_prefix(module.as_str())?.strip_prefix('.')?;
                Some((module.len(), alias, rest))
            })
            .max_by_key(|(len, ..)| *len);
        match renamed {
            Some((_, alias, rest)) => {
                self.push(alias);
                self.push(".");
                self.push(rest);
            }
            None => self.push(name),
        }
        Ok(())
    }

    /// A finite number; a negative one as `(-n)`. An integral one up to
    /// 2^53 is an integer literal, of any numeric type its use asks for.
    fn number(&mut self, n: f64) -> Result<(), String> {
        if !n.is_finite() {
            return Err(format!("{n} is no number of Flick"));
        }
        let digits = if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
            format!("{:.0}", n.abs())
        } else {
            format!("{:?}", n.abs())
        };
        let (tokens, errors) = lex(&digits, FileId::default());
        if !errors.is_empty() || tokens.len() != 1 || tokens[0].kind != K::Number {
            return Err(format!("{n} is no number of Flick"));
        }
        if n.is_sign_negative() {
            self.push("(-");
            self.push(&digits);
            self.push(")");
        } else {
            self.push(&digits);
        }
        Ok(())
    }

    fn string(&mut self, text: &str) {
        self.push("\"");
        for c in text.chars() {
            match c {
                '"' => self.push("\\\""),
                '\\' => self.push("\\\\"),
                '\n' => self.push("\\n"),
                '\t' => self.push("\\t"),
                '\r' => self.push("\\r"),
                c if c.is_control() => self.push(&format!("\\u{{{:x}}}", u32::from(c))),
                c => self.text.push(c),
            }
        }
        self.push("\"");
    }

    fn push(&mut self, s: &str) {
        self.text.push_str(s);
    }

    fn offset(&self) -> u32 {
        u32::try_from(self.text.len()).expect("generated code fits in u32")
    }
}

/// What a name may be where it stands.
#[derive(Clone, Copy)]
enum Names {
    /// A value, maybe qualified: `x`, `List.map`, `Just`, `Maybe.Just`.
    Value,
    /// A type or a type variable: `Int`, `Sql.Table`, `a`.
    Type,
    /// A constructor, maybe qualified.
    Constructor,
    /// A record label or a variable.
    Label,
    /// A module by its path: `Ui`, `Ui.Event`.
    Module,
}

impl Names {
    fn accept(self, name: &str) -> bool {
        let (tokens, errors) = lex_generated(name, FileId::default());
        if !errors.is_empty() || tokens.is_empty() {
            return false;
        }
        let kinds: Vec<K> = tokens.iter().map(|t| t.kind).collect();
        let (last, path) = kinds.split_last().expect("a token");
        // `Con.Con.` before the last part
        let qualified = path.len() % 2 == 0
            && path
                .chunks(2)
                .all(|pair| pair[0] == K::Con && pair[1] == K::Dot);
        match self {
            Names::Label => kinds == [K::Ident],
            Names::Value | Names::Type => qualified && matches!(last, K::Ident | K::Con),
            Names::Constructor | Names::Module => qualified && *last == K::Con,
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Names::Value => "a name",
            Names::Type => "a type name",
            Names::Constructor => "a constructor name",
            Names::Label => "a lowercase name",
            Names::Module => "a module",
        }
    }
}
