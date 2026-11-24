//! Code generation for inline Go foreign bindings and `foreign module inline`
//! preambles, including hoisted `import` declarations and `//line` directives.

use std::collections::BTreeSet;

use flick_backend_support::inline_span::map_offset;
use flick_ir::{Foreign, ForeignMode, InlineCode, Span};
use flick_pretty::{Document, DocumentArena, docvec};

use super::foreign::parameters;
use super::items::Decls;
use super::layout::{block, call, directive, signature, statement};
use super::{Gen, RUNTIME_PACKAGE, names};
use crate::inline::split_preamble;

/// Multi-line inline Go text emitted line-by-line via `a.line()` at nesting 0
/// so `named_again` line counters stay exact without adding spurious tabs.
fn raw_lines<'a>(a: &'a DocumentArena<'a>, code: &str) -> Document<'a> {
    let trimmed = code.trim_end_matches('\n');
    let lines = trimmed.split('\n').map(|line| a.text(a.alloc_str(line)));
    a.join(lines, a.line()).set_nesting(a, 0)
}

impl<'a> Gen<'a> {
    /// Resolves `(path, 1-based line)` for `offset` inside an inline literal `span`.
    fn inline_line(&self, span: Span, offset: usize) -> Option<(&'a str, u32)> {
        let path = self.input.paths.get(self.module)?;
        let file_idx = span.file.index() as usize;
        let starts = *self.input.lines.get(file_idx)?;
        let byte = self
            .input
            .sources
            .get(file_idx)
            .map_or(span.start, |src| map_offset(src, span, offset).start);
        let line = starts.partition_point(|&start| start <= byte);
        let line = u32::try_from(line).expect("fewer than 2^32 lines");
        Some((path.as_str(), line.max(1)))
    }

    /// Emits deduplicated hoisted `import` lines and package-level preamble bodies.
    pub(super) fn preambles(&mut self, preambles: &[InlineCode]) -> (Vec<Document<'a>>, Decls<'a>) {
        let a = self.a;
        let mut seen_imports: BTreeSet<(Option<String>, String)> = BTreeSet::new();
        seen_imports.insert((None, RUNTIME_PACKAGE.to_owned()));
        let mut import_docs = Vec::new();
        let mut decls = Vec::new();
        for preamble in preambles {
            let parsed = split_preamble(&preamble.code);
            for spec in parsed.imports {
                if !seen_imports.insert((spec.alias.clone(), spec.path.clone())) {
                    continue;
                }
                let pkg_ident = spec
                    .alias
                    .as_deref()
                    .unwrap_or_else(|| spec.path.rsplit('/').next().unwrap_or(&spec.path));
                self.go_imports
                    .entry(spec.path.clone())
                    .or_insert_with(|| pkg_ident.to_owned());
                self.go_import_idents.insert(pkg_ident.to_owned());
                let path_lit = names::string(&spec.path);
                let doc = match spec.alias {
                    Some(alias) => docvec![
                        a,
                        "import ",
                        a.alloc_str(&alias),
                        " ",
                        a.alloc_str(&path_lit)
                    ],
                    None => docvec![a, "import ", a.alloc_str(&path_lit)],
                };
                import_docs.push(doc);
            }
            let body = &preamble.code[parsed.body_offset..];
            if body.trim().is_empty() {
                continue;
            }
            let (doc, located) = match self.inline_line(preamble.span, parsed.body_offset) {
                Some((path, line)) => {
                    self.lines += 1;
                    (
                        docvec![a, "//line ", path, ":", line, a.line(), raw_lines(a, body)]
                            .set_nesting(a, 0),
                        true,
                    )
                }
                None => (raw_lines(a, body), false),
            };
            decls.push((doc, located));
        }
        (import_docs, decls)
    }

    /// Emits a `foreign ... from inline "..."` binding as either a package-level
    /// `var` (arity 0 pure constant) or a wrapper function binding the inline Go
    /// expression to `flickInlineFn` so Go ASI never rejects leading newlines or
    /// multi-line function literals.
    pub(super) fn inline_foreign(&mut self, f: &Foreign, inline: &InlineCode) -> Document<'a> {
        let a = self.a;
        let taken = parameters(f);
        let dir = self.inline_line(inline.span, 0).map(|(path, line)| {
            self.lines += 1;
            directive(a, path, line)
        });
        let rhs = match dir {
            Some(dir) => docvec![
                a,
                dir,
                a.line().set_nesting(a, 0),
                raw_lines(a, &inline.code)
            ],
            None => docvec![a, a.line().set_nesting(a, 0), raw_lines(a, &inline.code)],
        };
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return docvec![a, "var ", var, " flickrt.Value =", rhs];
        }
        let heads: Vec<&'a str> = (0..f.dicts)
            .map(|i| a.alloc_str(&format!("d{i}")))
            .chain((0..taken).map(|i| a.alloc_str(&format!("a{i}"))))
            .collect();
        let first = match f.mode {
            ForeignMode::Pure | ForeignMode::Sync | ForeignMode::Effect => "fb",
            ForeignMode::Async => "ctx",
        };
        let count = if f.arity == 0 { 0 } else { taken };
        let call_args = self.local_call_args(f, first, &heads, count);
        let go = call(a, "flickInlineFn", call_args);
        let go = self.wrap_foreign_result(f, go, false);
        let name = names::func(self.module, &f.name);
        let body = block(
            a,
            [
                statement(a, docvec![a, "flickInlineFn :=", rhs]),
                statement(a, docvec![a, "return ", go]),
            ],
        );
        docvec![
            a,
            "func ",
            name,
            "(",
            signature(&heads),
            ") flickrt.Value ",
            body
        ]
    }
}
