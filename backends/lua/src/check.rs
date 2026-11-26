//! The rules of a Lua profile over a request: the environment names a
//! profile, the dialect and capabilities are the host's, and what the
//! program reaches exists there. Every refusal is `P012`.

use std::collections::BTreeSet;

use flick_backend::{BuildRequest, Diagnostic, ModuleRole, Span};
use flick_ir::{Binding, Foreign, ForeignMode, Program};

use crate::kernel::{self, Table};
use crate::profiles::{self, HostProfile, PROFILES};
use crate::reach::{self, Fact, Reached, Wide};
use crate::refusal;

struct Rules<'a> {
    profile: &'static HostProfile,
    target: String,
    tags: &'a [String],
    kernel: Table<'static>,
    /// The built-in modules, whose JS globals of the kernel are the Lua
    /// kernel's.
    builtin: BTreeSet<&'a str>,
}

/// Checks `request` against its profile; the program and what it reaches.
pub(crate) fn check(request: &BuildRequest) -> Result<(Program, Reached), Vec<Diagnostic>> {
    let platform = request.target.environment.platform.as_str();
    let Some(profile) = profiles::find(platform) else {
        let known: Vec<String> = PROFILES
            .iter()
            .map(|p| format!("`lua/{}`", p.platform))
            .collect();
        return Err(vec![refusal(
            format!(
                "the profile `lua/{platform}` is not available on the lua target; its profiles are {}",
                known.join(", ")
            ),
            None,
            vec![],
        )]);
    };
    let rules = Rules {
        profile,
        target: format!("lua/{platform}"),
        tags: &request.target.tags,
        kernel: kernel::table(),
        builtin: (request.modules.iter())
            .filter(|m| m.role == ModuleRole::Builtin)
            .map(|m| m.name.as_str())
            .collect(),
    };
    let mut errors = rules.selection();
    if !errors.is_empty() {
        return Err(errors);
    }
    let program = Program::from_cbor(&request.program)
        .map_err(|e| vec![refusal(e.to_string(), None, vec![])])?;
    let reached = reach::walk(&program, request, &mut |fact, site| {
        let Some(message) = rules.refuse(&fact) else {
            return;
        };
        if errors.iter().any(|d| d.message == message) {
            return;
        }
        let span = Span {
            file: site.span.file.index(),
            start: site.span.start,
            end: site.span.end,
        };
        let notes = site.via.map(|via| format!("reached through `{via}`"));
        errors.push(refusal(message, Some(span), notes.into_iter().collect()));
    });
    if errors.is_empty() {
        return Ok((program, reached));
    }
    errors.sort_by_key(|d| d.span.as_ref().map(|s| (s.file, s.start, s.end)));
    Err(errors)
}

impl Rules<'_> {
    fn has(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    fn dialect(&self) -> &'static str {
        profiles::dialect(self.tags).tag
    }

    /// The dialect and capabilities the environment selects.
    fn selection(&self) -> Vec<Diagnostic> {
        let (profile, target) = (self.profile, &self.target);
        let dialect = self.dialect();
        let mut errors = Vec::new();
        if !profile.dialects.contains(&dialect) {
            errors.push(refusal(
                format!(
                    "the dialect `{dialect}` is not available on the {target} target: {} runs {}",
                    profile.title,
                    profile.dialects.join(", ")
                ),
                None,
                vec![],
            ));
        }
        for capability in ["jit", "ffi"] {
            if !self.has(capability) {
                continue;
            }
            let why = if !profile.luajit {
                format!("{} has no LuaJIT", profile.title)
            } else if dialect != "luajit" {
                "it needs the dialect `luajit`".to_owned()
            } else {
                continue;
            };
            errors.push(refusal(
                format!(
                    "the `{capability}` capability is not available on the {target} target: {why}"
                ),
                None,
                vec![],
            ));
        }
        errors
    }

    /// Why the profile cannot run a reached fact, if it cannot.
    fn refuse(&self, fact: &Fact) -> Option<String> {
        let target = &self.target;
        match fact {
            Fact::Number(Wide::BigInt) => Some(format!(
                "`BigInt` is not available on the {target} target yet: the Lua kernel has no \
                 arbitrary-precision integers"
            )),
            Fact::Number(wide) => (!self.has("ffi")).then(|| {
                format!(
                    "`{}` is not available on the {target} target without the `ffi` capability: \
                     a Lua number is a double, which would round it",
                    wide.name()
                )
            }),
            Fact::Await => self.suspension("`await`"),
            Fact::Foreign { module, foreign } => self.foreign(module, foreign),
        }
    }

    fn suspension(&self, what: &str) -> Option<String> {
        (!self.profile.suspension).then(|| {
            format!(
                "asynchronous suspension ({what}) is not available on the {} target: code runs to \
                 completion inside each host callback",
                self.target
            )
        })
    }

    fn foreign(&self, module: &str, foreign: &Foreign) -> Option<String> {
        let (profile, target) = (self.profile, &self.target);
        let what = format!(
            "`{module}.{}`",
            foreign.source_name.as_ref().unwrap_or(&foreign.name)
        );
        if foreign.mode == ForeignMode::Async && !profile.suspension {
            return self.suspension(&what);
        }
        match &foreign.binding {
            Binding::Inline(_) => None,
            Binding::Module { from, name } if from.starts_with("flick:") => {
                let row = self.kernel.row(from, name);
                if row.is_some_and(|r| r.implemented) {
                    return None;
                }
                if row.is_some_and(|r| r.suspends) && !profile.suspension {
                    return self.suspension(&what);
                }
                Some(format!(
                    "{what} (foreign `{name}` from `{from}`) is not available on the {target} target \
                     yet: the Lua kernel does not implement it"
                ))
            }
            Binding::Module { from, name } => {
                if !profile.require {
                    Some(format!(
                        "{what} (foreign `{name}` from `{from}`) is not available on the {target} \
                         target: {} has no `require`; an addon loads the files its .toc lists",
                        profile.title
                    ))
                } else if from == "ffi" && !self.has("ffi") {
                    Some(format!(
                        "{what} (foreign `{name}` from `{from}`) needs the `ffi` capability on the \
                         {target} target"
                    ))
                } else {
                    None
                }
            }
            Binding::Global { name }
                if self.builtin.contains(module) && self.kernel.row("global", name).is_some() =>
            {
                let implemented = self
                    .kernel
                    .row("global", name)
                    .is_some_and(|r| r.implemented);
                (!implemented).then(|| {
                    format!(
                        "{what} (the kernel's global `{name}`) is not available on the {target} \
                         target yet: the Lua kernel does not implement it"
                    )
                })
            }
            Binding::Global { name } => {
                let root = name.split('.').next().unwrap_or(name);
                if let Some((_, tag)) = profile.needs.iter().find(|(r, _)| *r == root)
                    && !self.has(tag)
                {
                    Some(format!(
                        "{what} (foreign `{name}` global) needs `{tag}` on the {target} target"
                    ))
                } else if !profile.defines(name) {
                    Some(format!(
                        "{what} (foreign `{name}` global) is not available on the {target} target: \
                         it is not in the API snapshot of {} (the profile `{}`)",
                        profile.title, profile.id
                    ))
                } else {
                    None
                }
            }
        }
    }
}
