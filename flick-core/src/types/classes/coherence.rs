//! Head-only overlap and functional-dependency consistency.

use super::declare::fundep_text;
use super::matching::{Subst, rename, resolve, unify_heads};
use super::{ClassInfo, split};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Checker;
use crate::types::env::Instance;
use crate::types::ty::Type;

impl Checker<'_> {
    /// Reports another instance of the class that this one overlaps or
    /// disagrees with on a functional dependency.
    pub(super) fn conflicts(
        &mut self,
        instance: &Instance,
        info: &ClassInfo,
        args: &[Type],
        span: Span,
    ) -> bool {
        let name = &instance.name;
        let class = split(&instance.head).map(|(k, _)| k);
        let others: Vec<Instance> = self
            .env
            .instances
            .iter()
            .filter(|i| split(&i.head).map(|(k, _)| k) == class)
            .cloned()
            .collect();
        for other in others {
            let names: Vec<String> = other.vars.iter().map(|v| v.name.clone()).collect();
            let theirs = rename(&other.head, &names);
            let Some((_, theirs)) = split(&theirs) else {
                continue;
            };
            let from = |other: &Instance| {
                if other.module == self.env.module {
                    String::new()
                } else {
                    format!(" of module `{}`", other.module)
                }
            };
            let mut disagreement = None;
            let mut incompatible = false;
            for fd in &info.fundeps {
                let mut subst = Subst::new();
                let same = fd
                    .from
                    .iter()
                    .all(|&i| unify_heads(&args[i], theirs[i], &mut subst));
                if !same {
                    continue;
                }
                if disagreement.is_none()
                    && fd
                        .to
                        .iter()
                        .any(|&j| resolve(&args[j], &subst) != resolve(theirs[j], &subst))
                {
                    disagreement = Some(fd);
                }
                if !fd
                    .to
                    .iter()
                    .all(|&j| unify_heads(&args[j], theirs[j], &mut subst))
                {
                    incompatible = true;
                    break;
                }
            }
            let mut subst = Subst::new();
            if !incompatible
                && theirs
                    .iter()
                    .zip(args)
                    .all(|(a, b)| unify_heads(a, b, &mut subst))
            {
                let common = self.show(&resolve(&instance.head, &subst));
                self.error(
                    codes::C003,
                    span,
                    format!(
                        "`{name}` overlaps `{}`{}: both apply to `{common}`, and instances may not overlap",
                        other.name,
                        from(&other)
                    ),
                );
                return true;
            }
            if let Some(fd) = disagreement {
                let dep = fundep_text(&info.params, fd);
                self.error(
                    codes::C004,
                    span,
                    format!(
                        "`{name}` and `{}`{} break the functional dependency `{dep}`: the same types determine different ones",
                        other.name,
                        from(&other)
                    ),
                );
                return true;
            }
        }
        false
    }
}
