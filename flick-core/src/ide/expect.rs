//! What the place of a completion expects, so that the candidates that fit
//! it come first: the type of an argument, of an annotated `let`'s body or
//! of a statement, the type a `match` pattern takes apart, the errors a
//! `try` can catch, and the services and errors a call there may need.
//! Only an editor's order: every candidate stays, and the checker decides
//! what the program means.

use super::complete::CompletionKind;
use super::hints::{binder, signed, strip};
use super::{Ide, tree};
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{Effects, Elem, Type};

/// What may stand where a completion goes.
pub(super) struct Expected<'a> {
    file: FileId,
    /// The types a value or pattern there may have; empty when unknown.
    types: Vec<Type>,
    /// A pattern, where only a constructor fits.
    pattern: bool,
    calls: Calls<'a>,
}

/// What a call at the place may need.
enum Calls<'a> {
    /// Not known: a `fun`, a `task` or a handler around it decides.
    Any,
    /// Nothing at runtime: a pure function's body, or a module's value.
    Pure,
    /// The services and errors of the runtime stage the place is in.
    Runtime(&'a Effects),
}

/// How far behind the candidates that fit a candidate that does not goes:
/// each keeps its group (locals, the module's own names …) among its kind.
pub(super) const MISFIT: u8 = 5;

impl<'a> Ide<'a> {
    /// What the place at `offset` of `file` expects: at a word that starts
    /// there, or, after a space, at the hole the cursor stands in.
    pub(super) fn expected(&self, file: FileId, offset: u32) -> Option<Expected<'a>> {
        let m = self.analysis.by_file(file)?;
        let text = &self.sources.get(file).text;
        let before = hole(text, offset);
        let path = tree::path_to(&m.tree, before.unwrap_or(offset));
        let (types, pattern) = match before {
            Some(before) => after(m, &path, before, text),
            None => around(m, &path, text),
        };
        let calls = calls(m, &path, text);
        if types.is_empty() && !pattern && matches!(calls, Calls::Any) {
            return None;
        }
        Some(Expected {
            file,
            types,
            pattern,
            calls,
        })
    }
}

impl Expected<'_> {
    /// How far a candidate of `kind` and type `ty` goes back: 0 when it
    /// fits, as a value, applied to some of its arguments or, in a pattern,
    /// as a constructor; [`MISFIT`] otherwise, an unknown type included.
    pub(super) fn misfit(&self, ide: &Ide, kind: CompletionKind, ty: Option<&Type>) -> u8 {
        let fits = ty.is_some_and(|ty| {
            if self.pattern {
                return kind == CompletionKind::Constructor
                    && (self.types.iter()).any(|t| self.fits(ide, result(ty), t, 0));
            }
            if !matches!(
                kind,
                CompletionKind::Local | CompletionKind::Value | CompletionKind::Constructor
            ) {
                return false;
            }
            // each stage applied runs its call, which must be allowed here
            let mut rest = strip(ty);
            loop {
                let fits = if self.types.is_empty() {
                    !matches!(rest, Type::Fun { .. })
                } else {
                    !wildcard(rest) && self.types.iter().any(|t| self.fits(ide, rest, t, 0))
                };
                if fits {
                    return true;
                }
                let Type::Fun {
                    effects, result, ..
                } = rest
                else {
                    return false;
                };
                if !self.calls.allow(effects.as_deref()) {
                    return false;
                }
                rest = strip(result);
            }
        });
        if fits { 0 } else { MISFIT }
    }

    /// Whether a value of `found` may stand where `wanted` is expected, up
    /// to the type variables of either, through transparent aliases; tuples
    /// and records of any fields may.
    fn fits(&self, ide: &Ide, found: &Type, wanted: &Type, depth: u32) -> bool {
        let (found, wanted) = (strip(found), strip(wanted));
        if wildcard(found) || wildcard(wanted) {
            return true;
        }
        match (found, wanted) {
            (
                Type::Fun { param, result, .. },
                Type::Fun {
                    param: takes,
                    result: gives,
                    ..
                },
            ) => self.fits(ide, param, takes, depth) && self.fits(ide, result, gives, depth),
            (Type::App { head, arg }, Type::App { head: of, arg: to })
                if self.fits(ide, head, of, depth) =>
            {
                self.fits(ide, arg, to, depth)
            }
            (Type::Con { .. }, Type::Con { .. }) if found == wanted => true,
            (Type::Tuple(_), Type::Tuple(_)) | (Type::Record(_), Type::Record(_)) => true,
            _ => {
                depth < 8
                    && ((ide.expand_alias(self.file, found))
                        .is_some_and(|found| self.fits(ide, &found, wanted, depth + 1))
                        || (ide.expand_alias(self.file, wanted))
                            .is_some_and(|wanted| self.fits(ide, found, &wanted, depth + 1)))
            }
        }
    }
}

impl Calls<'_> {
    /// Whether the call of an arrow of `effects`, pure without them, may
    /// run here.
    fn allow(&self, effects: Option<&Effects>) -> bool {
        match (self, effects) {
            (_, None) | (Calls::Any, _) => true,
            (Calls::Pure, Some(_)) => false,
            (Calls::Runtime(at), Some(e)) => {
                within(&e.services, &at.services) && within(&e.errors, &at.errors)
            }
        }
    }
}

/// Whether each service or error `row` names is in `allowed`, or `allowed`
/// is open.
fn within(row: &Type, allowed: &Type) -> bool {
    let (Type::Row(row), Type::Row(allowed)) = (row, allowed) else {
        return true;
    };
    allowed.tail.is_some()
        || (row.entries.iter()).all(|(e, _)| {
            !matches!(e, Elem::Named(_)) || allowed.entries.iter().any(|(a, _)| a == e)
        })
}

/// Where the code before a hole at `offset` ends, when no word starts
/// there and only spaces stand between: the cursor is in that code's scope.
pub(super) fn hole(text: &str, offset: u32) -> Option<u32> {
    let bytes = text.as_bytes();
    let mut at = offset as usize;
    if at > bytes.len()
        || bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return None;
    }
    while at > 0 && matches!(bytes[at - 1], b' ' | b'\t') {
        at -= 1;
    }
    u32::try_from(at).ok()
}

/// A type that stands for any: a variable, a meta or an error.
fn wildcard(ty: &Type) -> bool {
    matches!(
        strip(ty),
        Type::Var(_) | Type::Meta(_) | Type::Skolem(_) | Type::Error
    )
}

/// What a function gives with all its arguments.
fn result(mut ty: &Type) -> &Type {
    while let Type::Fun { result, .. } = strip(ty) {
        ty = result;
    }
    strip(ty)
}

/// `ty` after `n` arguments: the effects of the arrow of the last one, if
/// any, and the rest.
fn peel(ty: &Type, n: usize) -> Option<(Option<&Effects>, &Type)> {
    let mut ty = strip(ty);
    let mut effects = None;
    for _ in 0..n {
        let Type::Fun {
            effects: e, result, ..
        } = ty
        else {
            return None;
        };
        effects = e.as_deref();
        ty = strip(result);
    }
    Some((effects, ty))
}

/// The type of an expression whose type the checker keeps at a name.
fn type_of<'m>(m: &'m Analyzed, node: &Node) -> Option<&'m Type> {
    match node.kind {
        K::ParenExpr => type_of(m, node.nodes().next()?),
        K::AppExpr => {
            let head = node.nodes().next()?;
            peel(type_of(m, head)?, node.nodes().count() - 1).map(|(_, t)| t)
        }
        K::BinExpr => m.typing.types.get(&node.range()).or_else(|| {
            let op = node.child_tokens().find(|t| t.kind.is_binary_op())?;
            m.typing.types.get(&(op.start, op.end))
        }),
        _ => m.typing.types.get(&node.range()),
    }
}

/// The payload type of `Option a`.
fn option_inner(ty: &Type) -> Option<Type> {
    let (head, args) = strip(ty).spine();
    match args[..] {
        [inner] if head.is_lang(Lang::Option) => Some(inner.clone()),
        _ => None,
    }
}

/// The expected type of the right side of `left orelse right` when `left`
/// already resolves to a concrete `Option a`.
fn orelse_expected(m: &Analyzed, node: &Node) -> Option<Type> {
    let lhs = node.nodes().next()?;
    let inner = option_inner(type_of(m, lhs)?)?;
    (!wildcard(&inner)).then_some(inner)
}

/// The type the argument `index` of `head` takes.
fn argument(m: &Analyzed, head: &Node, index: usize) -> Vec<Type> {
    let mut out = Vec::new();
    if let Some((_, Type::Fun { param, .. })) = type_of(m, head).and_then(|t| peel(t, index)) {
        out.push((**param).clone());
    }
    out
}

/// The number of parameters `let` declares.
fn params(decl: &Node) -> usize {
    (decl.nodes())
        .find(|n| n.kind == K::Params)
        .map_or(0, |p| p.nodes().count())
}

/// The type of the name `decl` binds, when a signature before it among the
/// children of `parent` or an annotation declares it, not what the body
/// being written makes it; and whether a signature does, rows included.
fn declared<'m>(
    m: &'m Analyzed,
    parent: &Node,
    decl: &Node,
    text: &str,
) -> Option<(&'m Type, bool)> {
    let name = binder(decl)?;
    let signed = signed(parent, decl, name.text(text), text);
    let ty = m.typing.types.get(&(name.start, name.end))?;
    (signed || decl.has_token(K::Colon)).then_some((ty, signed))
}

/// What the body of `decl`, under `parent`, gives, when it is declared.
fn body(m: &Analyzed, parent: &Node, decl: &Node, text: &str) -> Vec<Type> {
    let mut out = Vec::new();
    if let Some((ty, _)) = declared(m, parent, decl, text)
        && let Some((_, ty)) = peel(ty, params(decl))
    {
        out.push(ty.clone());
    }
    out
}

/// The types the patterns of the arms of `outer` match: what a `match`
/// takes apart, or the errors the calls in the body of a `try` may raise.
fn pattern(m: &Analyzed, outer: &Node) -> (Vec<Type>, bool) {
    let mut out: Vec<Type> = Vec::new();
    let Some(subject) = outer.nodes().next() else {
        return (out, true);
    };
    if outer.kind == K::MatchExpr {
        if let Some(ty) = type_of(m, subject) {
            out.push(ty.clone());
        }
        return (out, true);
    }
    let (start, end) = subject.range();
    for ((s, e), ty) in &m.typing.types {
        let mut ty = strip(ty);
        while let Type::Fun {
            effects, result, ..
        } = ty
        {
            // a row keeps each error with its type
            if let Some(Type::Row(errors)) = effects.as_ref().map(|e| &e.errors)
                && start <= *s
                && *e <= end
            {
                out.extend(errors.entries.iter().map(|(_, error)| error.clone()));
            }
            ty = strip(result);
        }
    }
    (out, true)
}

/// What the code around the word at the end of `path` expects of it.
fn around(m: &Analyzed, path: &[&Node], text: &str) -> (Vec<Type>, bool) {
    let mut i = path.len() - 1;
    while i > 0 {
        let (node, child) = (path[i - 1], path[i]);
        let first = node
            .nodes()
            .next()
            .is_some_and(|n| n.range() == child.range());
        let last = node
            .nodes()
            .last()
            .is_some_and(|n| n.range() == child.range());
        match node.kind {
            K::ParenExpr => {}
            K::Block if last => {}
            // a statement before the last one is dropped: only `Unit` may be,
            // but in `do` it runs through its monad
            K::Block if i < 2 || path[i - 2].kind != K::DoExpr => {
                return (vec![Type::lang(Lang::Unit)], false);
            }
            K::AppExpr if !first => {
                let index = (node.nodes().skip(1))
                    .take_while(|a| a.range().0 < child.range().0)
                    .count();
                let head = node.nodes().next().unwrap_or(node);
                return (argument(m, head, index), false);
            }
            K::BinExpr if !first && node.has_token(K::OrElse) => {
                if let Some(inner) = orelse_expected(m, node) {
                    return (vec![inner], false);
                }
            }
            K::LetDecl if i >= 2 && last && child.kind == K::Block => {
                return (body(m, path[i - 2], node, text), false);
            }
            K::MatchArm if i >= 2 && first => return pattern(m, path[i - 2]),
            _ => break,
        }
        i -= 1;
    }
    (Vec::new(), false)
}

/// What a hole after the code that ends at `before`, the end of `path`,
/// expects: the body after `=` of a `let`, the pattern after `|` of an
/// arm, the fallback after `orelse`, or the next argument of the call or
/// name that ends there.
fn after(m: &Analyzed, path: &[&Node], before: u32, text: &str) -> (Vec<Type>, bool) {
    let mut j = path.len() - 1;
    if path[j].kind == K::Error
        && j > 1
        && path[j - 1].kind == K::BinExpr
        && path[j - 1].has_token(K::OrElse)
        && path[j - 1].range().1 == before
    {
        j -= 1;
    }
    // `f x |` ends at `x`: its call takes the next argument
    while j > 1 && path[j - 1].kind == K::AppExpr && path[j - 1].range().1 == before {
        j -= 1;
    }
    let node = path[j];
    if j == 0 || node.range().1 != before {
        return (Vec::new(), false);
    }
    match node.kind {
        K::LetDecl => (body(m, path[j - 1], node, text), false),
        K::MatchArm => pattern(m, path[j - 1]),
        K::BinExpr if node.has_token(K::OrElse) => match orelse_expected(m, node) {
            Some(inner) => (vec![inner], false),
            None => around(m, &path[..=j], text),
        },
        K::AppExpr => {
            let head = node.nodes().next().unwrap_or(node);
            (
                argument(m, head, node.nodes().count().saturating_sub(1)),
                false,
            )
        }
        K::NameExpr => (argument(m, node, 0), false),
        _ => (Vec::new(), false),
    }
}

/// What a call at the end of `path` may need: the rows of the stage the
/// body of the function around it runs at, as its signature declares them.
fn calls<'m>(m: &'m Analyzed, path: &[&Node], text: &str) -> Calls<'m> {
    let mut i = path.len() - 1;
    while i > 0 {
        let node = path[i];
        match node.kind {
            K::FunExpr
            | K::TaskExpr
            | K::LazyExpr
            | K::ProvideExpr
            | K::ScopeExpr
            | K::DoExpr
            | K::ForExpr
            | K::LoopExpr
            | K::Members
            | K::Children
            | K::OpenExpr
            | K::PluginExpr
            | K::PluginDecl
            | K::PluginRest
            | K::PluginArgs => return Calls::Any,
            // the body of a `try` may raise what its arms catch
            K::TryExpr
                if path.get(i + 1).map(|c| c.range()) == node.nodes().next().map(Node::range) =>
            {
                return Calls::Any;
            }
            K::LetDecl => {
                let top = path[i - 1].kind == K::SourceFile;
                match (params(node), top) {
                    // a local value runs where the code around it does
                    (0, false) => {}
                    // a module's values are made as it loads, where nothing runs
                    (0, true) => return Calls::Pure,
                    (n, true) => {
                        let declared = declared(m, path[i - 1], node, text);
                        let stage = declared.filter(|(_, signed)| *signed);
                        return match stage.and_then(|(ty, _)| peel(ty, n)) {
                            Some((Some(e), _)) => Calls::Runtime(e),
                            Some((None, _)) => Calls::Pure,
                            None => Calls::Any,
                        };
                    }
                    (_, false) => return Calls::Any,
                }
            }
            _ => {}
        }
        i -= 1;
    }
    Calls::Any
}
