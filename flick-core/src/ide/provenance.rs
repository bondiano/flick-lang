//! Field provenance: which record type alias declares the field a label
//! reads. Records are structural, so equal labels relate two records in no
//! way: a label belongs to an alias only where the checker typed its record
//! as that alias (a projection, an accessor step, a literal, an update, a
//! pattern), where its record is read from a field a shown alias declares
//! with that alias, or where the alias writes it. Navigation, references
//! and renaming follow only that evidence.

use super::hints::strip;
use super::{Ide, Location, Symbol, tree};
use crate::iface::Namespace;
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Type};

/// A record type alias, by its module and name.
pub(super) type Owner = (String, String);

/// Why the record of a label is not shown to be one alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unproven {
    /// A record type written out, which records of any declaration with
    /// these fields share.
    Structural,
    /// A record with more fields than it shows, as a row-polymorphic helper
    /// reads.
    Open,
    /// An alias of a record that another module declares.
    Across,
    /// Not known: a type variable, an error or no record.
    Unknown,
}

impl Unproven {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Unproven::Structural => {
                "its record is written out, and records of other declarations with these fields are of its type"
            }
            Unproven::Open => {
                "its record is open: a row-polymorphic function reads the field of other records too"
            }
            Unproven::Across => "its record is an alias of a record another module declares",
            Unproven::Unknown => "its record is not known",
        }
    }
}

/// A field label at a position: what it reads and whether its owner is shown.
pub(super) struct Label {
    pub(super) range: (u32, u32),
    pub(super) name: String,
    pub(super) owner: Result<Owner, Unproven>,
}

impl Ide<'_> {
    /// The body of the transparent alias `name` of `module`.
    pub(super) fn alias_body(&self, module: &str, name: &str) -> Option<&Type> {
        if let Some(m) = self.analysis.module(module) {
            return m.typing.aliases.get(name).map(|(_, body)| body);
        }
        match &self
            .interface(module)?
            .find(Namespace::Type, name)?
            .definition
            .as_ref()?
            .form
        {
            Form::Alias { body, .. } => Some(body),
            _ => None,
        }
    }

    /// The alias that declares the field `label` of a record of type
    /// `record`, when the checker typed it as that alias: through aliases of
    /// it in the alias's own module.
    pub(super) fn owner_of(&self, record: &Type, label: &str) -> Result<Owner, Unproven> {
        let (module, mut name) = match strip(record).spine().0 {
            Type::Con { module, name } => (module, name),
            Type::Record(row) if row.tail.is_some() => return Err(Unproven::Open),
            Type::Record(_) => return Err(Unproven::Structural),
            _ => return Err(Unproven::Unknown),
        };
        for _ in 0..8 {
            let body = strip(self.alias_body(module, name).ok_or(Unproven::Unknown)?);
            match body.spine().0 {
                Type::Record(row) if row.tail.is_some() => return Err(Unproven::Open),
                Type::Record(row) if row.get(label).is_some() => {
                    return Ok((module.clone(), name.clone()));
                }
                Type::Con {
                    module: other,
                    name: next,
                } if other == module => name = next,
                Type::Con { .. } => return Err(Unproven::Across),
                _ => return Err(Unproven::Unknown),
            }
        }
        Err(Unproven::Unknown)
    }

    /// The alias that declares the label `label` at `range` of `m`, read of
    /// a record of type `record`: the alias the checker typed the record as
    /// or, where the record is read from a field of a shown alias, the alias
    /// that field is declared with.
    pub(super) fn owner_at(
        &self,
        m: &Analyzed,
        range: (u32, u32),
        record: &Type,
        label: &str,
    ) -> Result<Owner, Unproven> {
        (self.owner_of(record, label))
            .or_else(|why| self.declared_receiver(m, range, label, 0).ok_or(why))
    }

    /// The alias the field read just before the label at `range` is
    /// declared with, if that field's alias is shown and has `label`.
    fn declared_receiver(
        &self,
        m: &Analyzed,
        range: (u32, u32),
        label: &str,
        depth: u32,
    ) -> Option<Owner> {
        let before = previous_label(&m.tree, range)?;
        let record = m.typing.fields.get(&before)?;
        let name = self
            .text(m.file)
            .get(before.0 as usize..before.1 as usize)?;
        let (module, alias) = match self.owner_of(record, name) {
            Ok(owner) => owner,
            Err(_) if depth < 8 => self.declared_receiver(m, before, name, depth + 1)?,
            Err(_) => return None,
        };
        let declaring = self.analysis.module(&module)?;
        let field = declared_field(&declaring.tree, self.text(declaring.file), &alias, name)?;
        let written = field.nodes().next().filter(|t| t.kind == K::NameType)?;
        let (module, name) = declaring.typing.type_names.get(&written.range())?;
        let declared = Type::Con {
            module: module.clone(),
            name: name.clone(),
        };
        self.owner_of(&declared, label).ok()
    }

    /// The field label at `offset` of `file`: one the checker relates to a
    /// record, or one a record type writes.
    pub(super) fn label_at(&self, file: FileId, offset: u32) -> Option<Label> {
        let m = self.module(file)?;
        let text = self.text(file);
        if let Some((range, record)) = tree::innermost(m.typing.fields.iter(), offset) {
            let name = text.get(range.0 as usize..range.1 as usize)?.to_owned();
            let owner = self.owner_at(m, range, record, &name);
            return Some(Label { range, name, owner });
        }
        let path = tree::path_to(&m.tree, offset);
        let at = path.iter().rposition(|n| n.kind == K::RecordTypeField)?;
        let label = path[at].child_tokens().find(|t| t.kind == K::Ident)?;
        if !(label.start <= offset && offset <= label.end) {
            return None;
        }
        // only the record an alias is declares a field of its own
        let alias = match path.get(at.wrapping_sub(2)..at) {
            Some([decl, record])
                if decl.kind == K::TypeDecl
                    && alias_record(decl).is_some_and(|r| r.range() == record.range()) =>
            {
                tree::name_of(decl, K::Con, text)
                    .filter(|name| self.alias_body(&m.name, name).is_some())
            }
            _ => None,
        };
        let owner = alias
            .map(|name| (m.name.clone(), name.to_owned()))
            .ok_or(Unproven::Structural);
        Some(Label {
            range: (label.start, label.end),
            name: label.text(text).to_owned(),
            owner,
        })
    }

    /// The field at `offset` as a symbol, when its owner is shown.
    pub(super) fn field_symbol_at(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(Symbol, (u32, u32))> {
        let label = self.label_at(file, offset)?;
        let (module, record) = label.owner.ok()?;
        Some((
            Symbol::Field {
                module,
                record,
                label: label.name,
            },
            label.range,
        ))
    }

    /// Where the alias `record` of `module` writes its field `label`.
    pub(super) fn field_declaration(
        &self,
        module: &str,
        record: &str,
        label: &str,
    ) -> Option<Location> {
        let source = self.source(module)?;
        let field = declared_field(&source.tree, &source.text, record, label)?;
        let (start, end) = field_label(field)?;
        Some(Location {
            target: source.target,
            start,
            end,
        })
    }

    /// The labels `label` of `m` with the record the checker relates each
    /// to; past the module's own text, those of code its plugins generate.
    pub(super) fn labels_named<'m>(
        &self,
        m: &'m Analyzed,
        label: &str,
    ) -> Vec<((u32, u32), &'m Type)> {
        let text = (m.expansion.as_ref()).map_or_else(|| self.text(m.file), |e| e.text.as_str());
        (m.typing.fields.iter())
            .filter(|(r, _)| text.get(r.0 as usize..r.1 as usize) == Some(label))
            .map(|(r, t)| (*r, t))
            .collect()
    }
}

/// The record an alias declaration is: its body when that is a record type.
pub(super) fn alias_record(decl: &Node) -> Option<&Node> {
    decl.nodes().find(|n| n.kind == K::RecordType)
}

/// The field `label` the alias `record` writes in `root`.
pub(super) fn declared_field<'n>(
    root: &'n Node,
    text: &str,
    record: &str,
    label: &str,
) -> Option<&'n Node> {
    let decl = root
        .nodes()
        .find(|d| d.kind == K::TypeDecl && tree::name_of(d, K::Con, text) == Some(record))?;
    alias_record(decl)?
        .nodes()
        .find(|f| f.kind == K::RecordTypeField && tree::name_of(f, K::Ident, text) == Some(label))
}

/// The range of the label of a written field.
pub(super) fn field_label(field: &Node) -> Option<(u32, u32)> {
    field
        .child_tokens()
        .find(|t| t.kind == K::Ident)
        .map(|t| (t.start, t.end))
}

/// The label of the field read just before the label at `range`: of the
/// projection its own reads a field of, or of the accessor step before it.
fn previous_label(root: &Node, range: (u32, u32)) -> Option<(u32, u32)> {
    let path = tree::path_to(root, range.0);
    let (node, parent) = match &path[..] {
        [.., parent, node] => (*node, *parent),
        _ => return None,
    };
    if field_label(node) != Some(range) {
        return None;
    }
    match node.kind {
        K::FieldExpr => {
            let mut inner = node.nodes().next()?;
            while inner.kind == K::ParenExpr {
                inner = inner.nodes().next()?;
            }
            (inner.kind == K::FieldExpr)
                .then(|| field_label(inner))
                .flatten()
        }
        K::AccessorStep => {
            let steps: Vec<&Node> = (parent.nodes())
                .filter(|n| n.kind == K::AccessorStep)
                .collect();
            let at = steps.iter().position(|s| s.range() == node.range())?;
            field_label(steps.get(at.checked_sub(1)?)?)
        }
        _ => None,
    }
}
